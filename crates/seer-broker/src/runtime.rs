use std::collections::HashMap;
use std::env;
use std::io::{self, PipeWriter};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use seer_core::proto::{ServerMsg, codec};

use crate::lifecycle::{Lifecycle, RuntimeRecord, RuntimeState};
use crate::os_identity::OsIdentity;
use crate::runtime_paths::{
    allow_identity_traversal, create_private_directory, prepare_runtime_socket_path,
    runtime_socket_path,
};
use crate::runtime_supervisor::{remove_runtime_socket, spawn_supervisor, terminate_process};

const CONNECT_RETRIES: usize = 500;
const RETRY_INTERVAL: Duration = Duration::from_millis(10);
const READINESS_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) type ProcessTable = HashMap<String, RuntimeProcess>;
pub(super) type UserLocks = HashMap<String, Arc<Mutex<()>>>;

pub(super) struct RuntimeProcess {
    pub(super) child: Child,
    pub(super) _lifeline: PipeWriter,
    pub(super) socket_path: PathBuf,
    pub(super) generation: String,
}

pub(crate) struct RuntimeManager {
    binary: PathBuf,
    state_dir: PathBuf,
    os_users: HashMap<String, String>,
    default_identity: OsIdentity,
    lifecycle: Arc<Lifecycle>,
    processes: Arc<Mutex<ProcessTable>>,
    user_locks: Arc<Mutex<UserLocks>>,
}

impl RuntimeManager {
    pub(crate) fn new(state_dir: PathBuf, os_users: HashMap<String, String>) -> io::Result<Self> {
        let binary = runtime_binary()?;
        let default_identity = OsIdentity::resolve_process_account()?;
        let lifecycle = Arc::new(Lifecycle::new(state_dir)?);
        let state_dir = lifecycle.state_dir().to_owned();
        let processes = Arc::new(Mutex::new(HashMap::new()));
        let user_locks = Arc::new(Mutex::new(HashMap::new()));
        spawn_supervisor(
            Arc::downgrade(&processes),
            Arc::downgrade(&lifecycle),
            Arc::downgrade(&user_locks),
        );
        Ok(Self {
            binary,
            state_dir,
            os_users,
            default_identity,
            lifecycle,
            processes,
            user_locks,
        })
    }

    pub(crate) fn connect(&self, user_id: &str, person_name: &str) -> io::Result<UnixStream> {
        self.connect_with_retries(user_id, person_name, CONNECT_RETRIES)
    }

    pub(crate) fn connect_existing(
        &self,
        user_id: &str,
        person_name: &str,
    ) -> io::Result<UnixStream> {
        let (identity, socket_path) = self.identity_and_socket(user_id, person_name)?;
        identity.check_switch_rights()?;
        let user_lock = self.user_lock(user_id)?;
        let _user_guard = user_lock
            .lock()
            .map_err(|_| io::Error::other("runtime user lock is poisoned"))?;
        let _file_lock = self.lifecycle.lock(user_id)?;
        let record = self.running_record(user_id)?;
        connect_ready(&socket_path, &record.generation)
    }

    fn connect_with_retries(
        &self,
        user_id: &str,
        person_name: &str,
        retries: usize,
    ) -> io::Result<UnixStream> {
        let (identity, socket_path) = self.identity_and_socket(user_id, person_name)?;
        identity.check_switch_rights()?;
        let users_directory = self.state_dir.join("users");
        create_private_directory(&users_directory)?;
        let state_directory = self.lifecycle.user_state_dir(user_id)?;
        identity.prepare_directory(&state_directory)?;
        allow_identity_traversal(&users_directory, &identity)?;
        prepare_runtime_socket_path(&socket_path, &identity)?;

        let user_lock = self.user_lock(user_id)?;
        let _user_guard = user_lock
            .lock()
            .map_err(|_| io::Error::other("runtime user lock is poisoned"))?;
        let _file_lock = self.lifecycle.lock(user_id)?;
        if let Some(record) = self.lifecycle.load(user_id)? {
            if record.state == RuntimeState::Running {
                if let Ok(stream) = connect_ready(&socket_path, &record.generation) {
                    return Ok(stream);
                }
                self.retire_process_if_generation(user_id, &record.generation)?;
                let _ = remove_runtime_socket(&socket_path);
            }
        }

        let starting = self.lifecycle.starting(user_id)?;
        let generation = starting.generation.clone();
        let process = match self.spawn(
            &socket_path,
            &state_directory,
            user_id,
            &identity,
            &generation,
        ) {
            Ok(process) => process,
            Err(error) => return Err(self.fail_start(user_id, &generation, &socket_path, error)),
        };
        if let Err(error) = self.insert_process(user_id, process) {
            return Err(self.fail_start(user_id, &generation, &socket_path, error));
        }

        let stream = match connect_with_retry(&socket_path, retries).and_then(|mut stream| {
            validate_ready(&mut stream, &generation)?;
            Ok(stream)
        }) {
            Ok(stream) => stream,
            Err(error) => return Err(self.fail_start(user_id, &generation, &socket_path, error)),
        };
        if !self
            .lifecycle
            .mark_running_if_current(user_id, &generation)?
        {
            let error = io::Error::other("runtime generation changed before readiness");
            return Err(self.fail_start(user_id, &generation, &socket_path, error));
        }
        Ok(stream)
    }

    pub(crate) fn is_running(&self, user_id: &str, person_name: &str) -> bool {
        let result: io::Result<bool> = (|| {
            let (identity, socket_path) = self.identity_and_socket(user_id, person_name)?;
            identity.check_switch_rights()?;
            let user_lock = self.user_lock(user_id)?;
            let _user_guard = user_lock
                .lock()
                .map_err(|_| io::Error::other("runtime user lock is poisoned"))?;
            let _file_lock = self.lifecycle.lock(user_id)?;
            let Some(record) = self.lifecycle.load(user_id)? else {
                return Ok(false);
            };
            if record.state != RuntimeState::Running {
                return Ok(false);
            }
            Ok(connect_ready(&socket_path, &record.generation).is_ok())
        })();
        result.unwrap_or(false)
    }

    fn running_record(&self, user_id: &str) -> io::Result<RuntimeRecord> {
        let Some(record) = self.lifecycle.load(user_id)? else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "runtime record is missing",
            ));
        };
        if record.state != RuntimeState::Running {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "runtime is not running",
            ));
        }
        Ok(record)
    }

    fn identity_and_socket(
        &self,
        user_id: &str,
        person_name: &str,
    ) -> io::Result<(OsIdentity, PathBuf)> {
        let identity = self.identity(person_name)?;
        let socket_path = runtime_socket_path(user_id, &identity, &self.state_dir)?;
        Ok((identity, socket_path))
    }

    fn user_lock(&self, user_id: &str) -> io::Result<Arc<Mutex<()>>> {
        let mut locks = self
            .user_locks
            .lock()
            .map_err(|_| io::Error::other("runtime user locks are poisoned"))?;
        Ok(Arc::clone(
            locks
                .entry(user_id.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        ))
    }

    fn insert_process(&self, user_id: &str, process: RuntimeProcess) -> io::Result<()> {
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| io::Error::other("runtime process lock is poisoned"))?;
        processes.insert(user_id.to_owned(), process);
        Ok(())
    }

    fn retire_process_if_generation(&self, user_id: &str, generation: &str) -> io::Result<()> {
        let process = {
            let mut processes = self
                .processes
                .lock()
                .map_err(|_| io::Error::other("runtime process lock is poisoned"))?;
            if processes
                .get(user_id)
                .is_some_and(|process| process.generation == generation)
            {
                processes.remove(user_id)
            } else {
                None
            }
        };
        if let Some(process) = process {
            terminate_process(process)?;
        }
        Ok(())
    }

    fn fail_start(
        &self,
        user_id: &str,
        generation: &str,
        socket_path: &Path,
        error: io::Error,
    ) -> io::Error {
        if let Err(cleanup) = self.retire_process_if_generation(user_id, generation) {
            eprintln!("runtime cleanup failed for user {user_id}: {cleanup}");
        }
        if let Err(cleanup) = remove_runtime_socket(socket_path) {
            eprintln!("runtime socket cleanup failed for user {user_id}: {cleanup}");
        }
        if let Err(record_error) =
            self.lifecycle
                .mark_failed_if_current(user_id, generation, &error.to_string())
        {
            eprintln!("runtime failure record failed for user {user_id}: {record_error}");
        }
        error
    }

    fn spawn(
        &self,
        socket_path: &Path,
        state_directory: &Path,
        user_id: &str,
        identity: &OsIdentity,
        generation: &str,
    ) -> io::Result<RuntimeProcess> {
        let (reader, writer) = io::pipe()?;
        let mut command = Command::new(&self.binary);
        command
            .arg(socket_path)
            .arg(user_id)
            .arg(identity.shell())
            .arg(generation)
            .env("SEER_SNAPSHOT_DIR", state_directory)
            .current_dir(state_directory)
            .stdin(Stdio::from(reader));
        identity.apply(&mut command)?;
        let child = command.spawn().map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("failed to launch runtime for OS account: {error}"),
            )
        })?;
        Ok(RuntimeProcess {
            child,
            _lifeline: writer,
            socket_path: socket_path.to_owned(),
            generation: generation.to_owned(),
        })
    }

    fn identity(&self, person_name: &str) -> io::Result<OsIdentity> {
        let Some(os_user) = self.os_users.get(person_name) else {
            return Ok(self.default_identity.clone());
        };
        let identity = OsIdentity::resolve(os_user)?;
        if identity.uid() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "person {person_name} maps to privileged OS account {os_user}; \
                     refuse unsafe mappings"
                ),
            ));
        }
        Ok(identity)
    }
}

fn connect_ready(path: &Path, generation: &str) -> io::Result<UnixStream> {
    let mut stream = UnixStream::connect(path)?;
    validate_ready(&mut stream, generation)?;
    Ok(stream)
}

fn validate_ready(stream: &mut UnixStream, generation: &str) -> io::Result<()> {
    stream.set_read_timeout(Some(READINESS_TIMEOUT))?;
    let message = codec::decode::<_, ServerMsg>(stream)?;
    stream.set_read_timeout(None)?;
    match message {
        ServerMsg::RuntimeReady {
            generation: reported,
        } if reported == generation => Ok(()),
        ServerMsg::RuntimeReady { .. } => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime generation does not match",
        )),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime did not report readiness",
        )),
    }
}

fn runtime_binary() -> io::Result<PathBuf> {
    match env::var_os("SEER_RUNTIME_BIN") {
        Some(binary) => Ok(PathBuf::from(binary)),
        None => runtime_binary_next_to(&env::current_exe()?),
    }
}

fn runtime_binary_next_to(executable: &Path) -> io::Result<PathBuf> {
    executable
        .parent()
        .map(|directory| directory.join(format!("seer-runtime{}", env::consts::EXE_SUFFIX)))
        .ok_or_else(|| io::Error::other("broker executable has no parent directory"))
}

fn connect_with_retry(path: &Path, retries: usize) -> io::Result<UnixStream> {
    let mut last_error = match UnixStream::connect(path) {
        Ok(stream) => return Ok(stream),
        Err(error) => error,
    };
    for _ in 0..retries {
        thread::sleep(RETRY_INTERVAL);
        match UnixStream::connect(path) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

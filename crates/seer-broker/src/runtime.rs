use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs::{self, DirBuilder, Permissions};
use std::io::{self, PipeWriter};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::Duration;

use crate::os_identity::{OsIdentity, process_uid};

const CONNECT_RETRIES: usize = 500;
const RETRY_INTERVAL: Duration = Duration::from_millis(10);
const SUPERVISOR_INTERVAL: Duration = Duration::from_millis(250);
const MAX_SOCKET_PATH_BYTES: usize = 99;
struct RuntimeProcess {
    child: Child,
    _lifeline: PipeWriter,
    socket_path: PathBuf,
}

pub(crate) struct RuntimeManager {
    binary: PathBuf,
    state_dir: PathBuf,
    os_users: HashMap<String, String>,
    default_identity: OsIdentity,
    processes: Arc<Mutex<HashMap<String, RuntimeProcess>>>,
}

impl RuntimeManager {
    pub(crate) fn new(state_dir: PathBuf, os_users: HashMap<String, String>) -> io::Result<Self> {
        let default_identity = OsIdentity::resolve_process_account()?;
        let processes = Arc::new(Mutex::new(HashMap::new()));
        spawn_supervisor(Arc::downgrade(&processes));
        Ok(Self {
            binary: runtime_binary()?,
            state_dir,
            os_users,
            default_identity,
            processes,
        })
    }

    pub(crate) fn connect(&self, user_id: &str, person_name: &str) -> io::Result<UnixStream> {
        self.connect_with_retries(user_id, person_name, CONNECT_RETRIES)
    }

    fn connect_with_retries(
        &self,
        user_id: &str,
        person_name: &str,
        retries: usize,
    ) -> io::Result<UnixStream> {
        let identity = self.identity(person_name)?;
        identity.check_switch_rights()?;
        let users_directory = self.state_dir.join("users");
        create_private_directory(&users_directory)?;
        let state_directory = self.user_state_directory(user_id);
        identity.prepare_directory(&state_directory)?;
        allow_identity_traversal(&users_directory, &identity)?;
        let socket_path = runtime_socket_path(user_id, &identity, &self.state_dir)?;
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| io::Error::other("runtime process lock is poisoned"))?;

        if let Ok(stream) = UnixStream::connect(&socket_path) {
            return Ok(stream);
        }

        let process_is_running = match processes.get_mut(user_id) {
            Some(process) => runtime_is_running(user_id, process)?,
            None => false,
        };
        if !process_is_running {
            processes.remove(user_id);
            let process = self.spawn(&socket_path, &state_directory, user_id, &identity)?;
            processes.insert(user_id.to_owned(), process);
        }

        drop(processes);
        connect_with_retry(&socket_path, retries)
    }

    pub(crate) fn is_running(&self, user_id: &str, person_name: &str) -> bool {
        self.identity(person_name)
            .and_then(|identity| identity.check_switch_rights().map(|()| identity))
            .and_then(|identity| runtime_socket_path(user_id, &identity, &self.state_dir))
            .and_then(UnixStream::connect)
            .is_ok()
    }

    fn user_state_directory(&self, user_id: &str) -> PathBuf {
        self.state_dir.join("users").join(user_id)
    }

    fn spawn(
        &self,
        socket_path: &Path,
        state_directory: &Path,
        user_id: &str,
        identity: &OsIdentity,
    ) -> io::Result<RuntimeProcess> {
        let (reader, writer) = io::pipe()?;
        let mut command = Command::new(&self.binary);
        command
            .arg(socket_path)
            .arg(user_id)
            .arg(identity.shell())
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

fn spawn_supervisor(processes: Weak<Mutex<HashMap<String, RuntimeProcess>>>) {
    let _monitor = thread::spawn(move || {
        supervise_runtimes(processes);
    });
}

fn supervise_runtimes(processes: Weak<Mutex<HashMap<String, RuntimeProcess>>>) {
    loop {
        thread::sleep(SUPERVISOR_INTERVAL);
        let Some(processes) = processes.upgrade() else {
            return;
        };
        let mut processes = match processes.lock() {
            Ok(processes) => processes,
            Err(_) => {
                eprintln!("runtime supervisor lock failed");
                return;
            }
        };
        processes.retain(
            |user_id, process| match runtime_is_running(user_id, process) {
                Ok(running) => running,
                Err(error) => {
                    eprintln!("runtime status check failed for user {user_id}: {error}");
                    true
                }
            },
        );
    }
}

fn runtime_is_running(user_id: &str, process: &mut RuntimeProcess) -> io::Result<bool> {
    let Some(status) = process.child.try_wait()? else {
        return Ok(true);
    };
    if let Err(error) = remove_runtime_socket(&process.socket_path) {
        eprintln!("runtime socket cleanup failed for user {user_id}: {error}");
    }
    eprintln!("runtime exited for user {user_id}: {status}");
    Ok(false)
}

fn remove_runtime_socket(path: &Path) -> io::Result<()> {
    if !path.exists() || UnixStream::connect(path).is_ok() {
        return Ok(());
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn runtime_socket_path(user: &str, identity: &OsIdentity, state_dir: &Path) -> io::Result<PathBuf> {
    validate_user_id(user)?;
    let xdg_runtime_dir = env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty());
    let broker_uid = process_uid();
    let directory = runtime_directory_path(state_dir, xdg_runtime_dir, broker_uid, identity.uid());
    let parent = directory
        .parent()
        .ok_or_else(|| io::Error::other("runtime socket directory has no parent"))?;
    if !parent.exists() {
        create_private_directory(parent)?;
    }
    identity.prepare_directory(&directory)?;
    let path = directory.join(format!("{user}.sock"));
    validate_socket_path(&path)?;
    Ok(path)
}

fn allow_identity_traversal(directory: &Path, identity: &OsIdentity) -> io::Result<()> {
    let metadata = fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink() {
        return Err(unsafe_directory(directory));
    }
    if metadata.uid() != identity.uid() {
        // The broker keeps the shared users directory private. The
        // mapped account still needs to reach its own state directory
        // inside it, so grant traversal without listing rights.
        fs::set_permissions(directory, Permissions::from_mode(0o711))?;
    }
    Ok(())
}

fn unsafe_directory(directory: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("runtime directory is unsafe: {}", directory.display()),
    )
}

fn validate_socket_path(path: &Path) -> io::Result<()> {
    if path.as_os_str().as_bytes().len() > MAX_SOCKET_PATH_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime socket path exceeds 99 bytes",
        ));
    }
    Ok(())
}

fn runtime_directory_path(
    state_dir: &Path,
    xdg_runtime_dir: Option<OsString>,
    broker_uid: u32,
    target_uid: u32,
) -> PathBuf {
    match xdg_runtime_dir.filter(|value| !value.is_empty() && broker_uid == target_uid) {
        Some(root) => PathBuf::from(root).join("seer"),
        None => state_dir.join(format!("seer-{target_uid}")),
    }
}

fn validate_user_id(user: &str) -> io::Result<()> {
    let valid = !user.is_empty()
        && user.len() <= 64
        && user
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime user ID is unsafe",
        ))
    }
}

fn create_private_directory(directory: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true).mode(0o700).create(directory)?;
    fs::set_permissions(directory, Permissions::from_mode(0o700))
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::validate_socket_path;

    #[test]
    fn limits_socket_paths_to_less_than_one_hundred_bytes() {
        let allowed = Path::new("a").join("b".repeat(97));
        let rejected = Path::new("a").join("b".repeat(98));

        assert_eq!(allowed.as_os_str().len(), 99);
        validate_socket_path(&allowed).expect("99-byte socket path must be valid");
        assert_eq!(rejected.as_os_str().len(), 100);
        assert_eq!(
            validate_socket_path(&rejected)
                .expect_err("100-byte socket path must be invalid")
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
}

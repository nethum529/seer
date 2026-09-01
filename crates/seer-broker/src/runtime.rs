use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

const CONNECT_RETRIES: usize = 500;
const RETRY_INTERVAL: Duration = Duration::from_millis(10);
const MAX_SOCKET_PATH_BYTES: usize = 99;

pub(crate) struct RuntimeManager {
    binary: PathBuf,
    shell: String,
    state_dir: PathBuf,
    processes: Mutex<HashMap<String, Child>>,
}

impl RuntimeManager {
    pub(crate) fn new(shell: String, state_dir: PathBuf) -> io::Result<Self> {
        Ok(Self {
            binary: runtime_binary()?,
            shell,
            state_dir,
            processes: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) fn connect(&self, user_id: &str) -> io::Result<UnixStream> {
        self.connect_with_retries(user_id, CONNECT_RETRIES)
    }

    fn connect_with_retries(&self, user_id: &str, retries: usize) -> io::Result<UnixStream> {
        let socket_path = runtime_socket_path(user_id)?;
        let state_directory = self.user_state_directory(user_id);
        create_private_directory(&state_directory)?;
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| io::Error::other("runtime process lock is poisoned"))?;

        if let Ok(stream) = UnixStream::connect(&socket_path) {
            return Ok(stream);
        }

        let process_is_running = match processes.get_mut(user_id) {
            Some(process) => process.try_wait()?.is_none(),
            None => false,
        };
        if !process_is_running {
            processes.remove(user_id);
            let process = self.spawn(&socket_path, &state_directory, user_id)?;
            processes.insert(user_id.to_owned(), process);
        }

        drop(processes);
        connect_with_retry(&socket_path, retries)
    }

    pub(crate) fn is_running(&self, user_id: &str) -> bool {
        runtime_socket_path(user_id)
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
    ) -> io::Result<Child> {
        Command::new(&self.binary)
            .arg(socket_path)
            .arg(user_id)
            .arg(&self.shell)
            .current_dir(state_directory)
            .stdin(Stdio::null())
            .spawn()
    }
}

fn runtime_socket_path(user: &str) -> io::Result<PathBuf> {
    let xdg_runtime_dir = env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty());
    let uid = if xdg_runtime_dir.is_some() {
        0
    } else {
        current_uid()?
    };
    let directory = runtime_directory_path(xdg_runtime_dir, uid);
    create_private_directory(&directory)?;
    let path = directory.join(format!("{user}.sock"));
    validate_socket_path(&path)?;
    Ok(path)
}

#[cfg(test)]
pub(crate) fn runtime_socket_path_for_test(user: &str) -> io::Result<PathBuf> {
    runtime_socket_path(user)
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

fn runtime_directory_path(xdg_runtime_dir: Option<OsString>, uid: u32) -> PathBuf {
    match xdg_runtime_dir.filter(|value| !value.is_empty()) {
        Some(root) => PathBuf::from(root).join("seer"),
        None => PathBuf::from(format!("/tmp/seer-{uid}")),
    }
}

fn create_private_directory(directory: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true).mode(0o700).create(directory)?;
    fs::set_permissions(directory, Permissions::from_mode(0o700))
}

fn current_uid() -> io::Result<u32> {
    let output = Command::new("id").arg("-u").output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "id -u failed with status {}",
            output.status
        )));
    }
    let uid = str::from_utf8(&output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        .trim()
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(uid)
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
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::path::Path;
    use std::process::Command;
    use std::sync::Mutex;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::{
        RuntimeManager, connect_with_retry, create_private_directory, current_uid,
        runtime_binary_next_to, runtime_directory_path, validate_socket_path,
    };

    #[test]
    fn selects_runtime_directories_and_private_mode() {
        let temporary = temporary_directory("directories");
        let xdg_root = temporary.join("xdg");
        let xdg = runtime_directory_path(Some(xdg_root.clone().into_os_string()), 123);
        let fallback = runtime_directory_path(Some(OsString::new()), 123);
        create_private_directory(&xdg).expect("XDG directory must be created");

        assert_eq!(xdg, xdg_root.join("seer"));
        assert_eq!(fallback, Path::new("/tmp/seer-123"));
        assert_eq!(mode(&xdg), 0o700);

        fs::remove_dir_all(temporary).expect("temporary directory must be removed");
    }

    #[test]
    fn finds_runtime_next_to_broker() {
        let path = runtime_binary_next_to(Path::new("/opt/seer/seer-broker"))
            .expect("runtime path must resolve");

        assert_eq!(path, Path::new("/opt/seer/seer-runtime"));
    }

    #[test]
    fn reads_the_process_user_id() {
        let uid = current_uid().expect("process user ID must load");
        let output = Command::new("id")
            .arg("-u")
            .output()
            .expect("id command must run");
        assert!(output.status.success(), "id command must succeed");
        let expected = String::from_utf8(output.stdout)
            .expect("id output must be UTF-8")
            .trim()
            .parse::<u32>()
            .expect("id output must be a user ID");

        assert_eq!(uid, expected);
    }

    #[test]
    fn rejects_broker_path_without_parent() {
        let error =
            runtime_binary_next_to(Path::new("/")).expect_err("root path has no parent directory");

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
    }

    #[test]
    fn retries_until_socket_is_ready() {
        let temporary = temporary_directory("retry");
        let socket = temporary.join("r.sock");
        let listener_path = socket.clone();
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            let _listener = UnixListener::bind(listener_path).expect("socket must bind");
            thread::sleep(Duration::from_millis(100));
        });

        let stream = connect_with_retry(&socket, 100).expect("delayed socket must connect");

        drop(stream);
        worker.join().expect("listener thread must finish");
        fs::remove_dir_all(temporary).expect("temporary directory must be removed");
    }

    #[test]
    fn retry_returns_last_connection_error() {
        let temporary = temporary_directory("timeout");
        let socket = temporary.join("m.sock");

        connect_with_retry(&socket, 2).expect_err("missing socket must fail");
        fs::remove_dir_all(temporary).expect("temporary directory must be removed");
    }

    #[test]
    fn records_a_spawned_runtime_process() {
        let temporary = temporary_directory("spawn");
        let binary = temporary.join("runtime-test");
        fs::write(&binary, "#!/bin/sh\nwhile :; do :; done\n")
            .expect("test runtime must be written");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))
            .expect("test runtime must be executable");
        let manager = RuntimeManager {
            binary,
            shell: "sh".into(),
            state_dir: temporary.clone(),
            processes: Mutex::new(HashMap::new()),
        };

        manager
            .connect_with_retries("spawn-test", 0)
            .expect_err("exited runtime must not open a socket");
        let mut processes = manager.processes.lock().expect("process lock must work");
        let child = processes
            .get_mut("spawn-test")
            .expect("spawned process must be recorded");
        wait_or_kill(child, Duration::from_millis(20));
        wait_or_kill(child, Duration::from_millis(20));
        drop(processes);
        fs::remove_dir_all(temporary).expect("temporary directory must be removed");
    }

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

    fn temporary_directory(name: &str) -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos()
            % 1_000_000_000;
        let path = Path::new("/tmp").join(format!("mb-{name}-{}-{timestamp}", std::process::id()));
        fs::create_dir(&path).expect("temporary directory must be created");
        path
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path)
            .expect("directory metadata must load")
            .permissions()
            .mode()
            & 0o777
    }

    fn wait_or_kill(child: &mut std::process::Child, timeout: Duration) {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if child.try_wait().expect("child status must load").is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        child.kill().expect("late child must stop");
        child.wait().expect("stopped child must be reaped");
    }
}

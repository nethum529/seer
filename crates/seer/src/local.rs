use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::version::major_minor;
use seer_net::{Socket, Stream};

use crate::store::ServerEntry;

const START_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const READY_TIMEOUT: Duration = Duration::from_secs(5);
const KILL_GRACE: Duration = Duration::from_secs(1);

// The runtime keeps running when this window closes and when the room stops.
pub(crate) fn attach(server: &ServerEntry) -> io::Result<Attached> {
    let directory = runtime_directory(&server.user_id)?;
    let socket = directory.join("socket");
    if let Ok(ready) = greet(&socket, None, READY_TIMEOUT) {
        return attach_stream(ready);
    }
    // Two windows opening together must share one runtime, not race to spawn
    // two and leave a stale PID behind.
    let _startup = StartupLock::hold(&directory.join("startup.lock"))?;
    if let Ok(ready) = greet(&socket, None, READY_TIMEOUT) {
        return attach_stream(ready);
    }
    let generation = generation();
    let mut child = start(&directory, &socket, server, &generation)?;
    let ready = connect_until_ready(&socket, &generation, &mut child, &directory)?;
    write_private(&directory.join("runtime.pid"), &child.id().to_string())?;
    attach_stream(ready)
}

// The local link, the terminal tree, and the standing notice for a runtime of
// another Seer version.
pub(crate) type Attached = (Socket, Tree, Option<String>);

type Ready = (UnixStream, Option<String>);

struct StartupLock(File);

impl StartupLock {
    fn hold(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;
        // SAFETY: flock takes a file descriptor this process owns and touches
        // no memory. The lock is released when the file closes.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(file))
    }
}

impl Drop for StartupLock {
    fn drop(&mut self) {
        // SAFETY: flock takes a file descriptor this process owns.
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

fn attach_stream((mut stream, version): Ready) -> io::Result<Attached> {
    codec::encode(&mut stream, &ClientMsg::AttachRuntime)?;
    let ServerMsg::Tree { tree } = codec::decode(&mut stream)? else {
        return Err(io::Error::other("the runtime did not send your terminals"));
    };
    let socket = Socket::from(stream);
    socket.set_read_timeout(None)?;
    Ok((socket, tree, runtime_notice(version.as_deref())))
}

// seer update replaces the binaries but stops no process, so a window can
// attach to a runtime from before the update. Seer does not stop it.
fn runtime_notice(version: Option<&str>) -> Option<String> {
    let own = major_minor(env!("CARGO_PKG_VERSION"));
    if version
        .and_then(major_minor)
        .is_some_and(|found| Some(found) == own)
    {
        return None;
    }
    Some(
        "Your terminals on this computer run an older Seer (or another version). Run: seer restart. The restart ends the running shells."
            .to_owned(),
    )
}

// The user ID is minted for one person in one room, so it scopes the local
// state to both. Another room cannot reach these terminals.
pub(crate) fn runtime_directory(user_id: &str) -> io::Result<PathBuf> {
    let directory = state_dir()?.join("runtimes").join(user_id);
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    Ok(directory)
}

pub(crate) fn state_dir() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("seer"));
    }
    let home = env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(".local/state/seer"))
}

pub(crate) fn connect(socket: &Path, generation: Option<&str>) -> io::Result<UnixStream> {
    connect_within(socket, generation, READY_TIMEOUT)
}

pub(crate) fn connect_within(
    socket: &Path,
    generation: Option<&str>,
    ready_timeout: Duration,
) -> io::Result<UnixStream> {
    greet(socket, generation, ready_timeout).map(|(stream, _)| stream)
}

fn greet(socket: &Path, generation: Option<&str>, ready_timeout: Duration) -> io::Result<Ready> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(ready_timeout))?;
    match codec::decode::<_, ServerMsg>(&mut stream)? {
        ServerMsg::RuntimeReady {
            generation: found,
            version,
        } if generation.is_none_or(|wanted| wanted == found) => {
            stream.set_read_timeout(None)?;
            Ok((stream, version))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "another runtime holds this socket",
        )),
    }
}

fn connect_until_ready(
    socket: &Path,
    generation: &str,
    child: &mut Child,
    directory: &Path,
) -> io::Result<Ready> {
    let deadline = Instant::now() + START_TIMEOUT;
    let mut last = io::Error::new(io::ErrorKind::TimedOut, "the runtime did not start");
    while Instant::now() < deadline {
        match greet(socket, Some(generation), READY_TIMEOUT) {
            Ok(ready) => return Ok(ready),
            Err(error) => last = error,
        }
        if let Some(status) = child.try_wait()? {
            return Err(runtime_stopped(status, directory));
        }
        thread::sleep(POLL_INTERVAL);
    }
    Err(last)
}

// Issue 362: a runtime that stops before it opens its socket must report that
// failure. The missing socket alone reads as "No such file or directory" and
// hides the reason.
fn runtime_stopped(status: ExitStatus, directory: &Path) -> io::Error {
    io::Error::other(format!(
        "the runtime stopped before it opened its socket ({status}); see {}",
        directory.join("runtime.log").display()
    ))
}

fn start(
    directory: &Path,
    socket: &Path,
    server: &ServerEntry,
    generation: &str,
) -> io::Result<Child> {
    let binary = find_runtime()?;
    let log = open_log(&directory.join("runtime.log"))?;
    let stderr = log.try_clone()?;
    let mut command = Command::new(binary);
    command
        .arg(socket)
        .arg(&server.user_id)
        .arg(shell())
        .arg(generation)
        .env("SEER_SNAPSHOT_DIR", directory)
        .env("SEER_ROOM_ENDPOINT", &server.endpoint)
        .env("SEER_ROOM_CREDENTIAL", &server.credential)
        .env("SEER_ROOM_KEY", directory.join("runtime.key"))
        .current_dir(start_directory())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr));
    // SAFETY: setsid has no memory safety requirement and runs before exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() >= 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
    command.spawn()
}

fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents.as_bytes())
}

fn start_directory() -> PathBuf {
    env::current_dir()
        .ok()
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn shell() -> String {
    env::var("SHELL")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_owned())
}

fn generation() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{}-{nanos}", std::process::id())
}

fn open_log(path: &Path) -> io::Result<File> {
    let log = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path)?;
    log.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(log)
}

fn find_runtime() -> io::Result<PathBuf> {
    let executable = env::current_exe()?;
    if let Some(candidate) = executable
        .parent()
        .map(|parent| parent.join("seer-runtime"))
        && candidate.is_file()
    {
        return Ok(candidate);
    }
    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            let candidate = directory.join("seer-runtime");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "seer-runtime was not found",
    ))
}

pub(crate) fn runtime_answers(user_id: &str) -> bool {
    runtime_directory(user_id)
        .is_ok_and(|directory| connect(&directory.join("socket"), None).is_ok())
}

// This reaches only the shells on this computer. Another participant's shells
// run on their own computer.
pub(crate) fn stop(user_id: &str) -> io::Result<bool> {
    let directory = state_dir()?.join("runtimes").join(user_id);
    let socket = directory.join("socket");
    if connect(&socket, None).is_err() {
        return Ok(false);
    }
    let Some(pid) = read_pid(&directory.join("runtime.pid")) else {
        return Ok(false);
    };
    stop_pid(pid, || connect(&socket, None).is_ok())?;
    Ok(true)
}

// A negative pid names a process group.
pub(crate) fn stop_pid(pid: i32, alive: impl Fn() -> bool) -> io::Result<()> {
    signal(pid, libc::SIGTERM)?;
    if wait_while(&alive, START_TIMEOUT) {
        signal(pid, libc::SIGKILL)?;
        // A killed process stays visible until its parent reaps it.
        wait_while(&alive, KILL_GRACE);
    }
    Ok(())
}

fn wait_while(condition: &impl Fn() -> bool, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while condition() {
        if Instant::now() >= deadline {
            return true;
        }
        thread::sleep(POLL_INTERVAL);
    }
    false
}

pub(crate) fn process_alive(pid: i32) -> bool {
    // SAFETY: kill with signal 0 only checks that the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn read_pid(path: &Path) -> Option<i32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn signal(pid: i32, number: i32) -> io::Result<()> {
    // SAFETY: kill takes a process ID and a signal number and touches no memory.
    if unsafe { libc::kill(pid, number) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(test)]
mod tests;

#![cfg(target_os = "linux")]

use std::fs;
use std::net::TcpListener;
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[path = "support/runtimes.rs"]
mod runtimes;

const WAIT: Duration = Duration::from_secs(20);
const EXIT_WAIT: Duration = Duration::from_secs(5);
const USER: &str = "alice-user-id";

// Issue 338: several windows on one computer share that person's runtime, and
// a window opens even when the room is unreachable. A race between two windows
// must not leave two runtimes or a stale PID.
#[test]
fn windows_opening_together_share_one_runtime_while_the_room_is_offline() {
    let root = test_root("seer-local");
    write_store(&root);

    let _windows = Windows {
        root: root.clone(),
        children: (0..4).map(|_| open_window(&root)).collect(),
    };
    let socket = socket_path(&root);
    let first = read_tree(&socket);
    for _ in 0..3 {
        assert_eq!(
            read_tree(&socket),
            first,
            "every window on this computer must attach to the same runtime"
        );
    }

    let directory = runtime_directory(&root);
    let pid = wait_for_pid(&directory);
    assert!(
        Path::new(&format!("/proc/{pid}")).exists(),
        "the recorded PID must be the runtime that is running, not one that lost the race"
    );
}

// Issue 362: a runtime that stops before it opens its socket must be reported
// as a runtime failure. The missing socket alone reads as a missing file.
#[test]
fn a_runtime_that_stops_early_reports_the_runtime_failure() {
    let root = test_root("seer-runtime-stopped");
    let _cleanup = CleanOnDrop(root.clone());
    write_store(&root);
    let directory = runtime_directory(&root);
    fs::create_dir_all(directory.join("socket")).expect("blocked socket must be created");

    let output = run_window_to_end(&root);

    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(
        text.contains("the runtime stopped before it opened its socket"),
        "{text}"
    );
    assert!(text.contains("runtime.log"), "{text}");
    assert!(!text.contains("No such file or directory"), "{text}");
}

// The window ends on its own here, so nothing else removes the scratch
// directory, and a failed assertion would leave it behind.
struct CleanOnDrop(PathBuf);

impl Drop for CleanOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Windows {
    root: PathBuf,
    children: Vec<Child>,
}

impl Drop for Windows {
    fn drop(&mut self) {
        for child in &mut self.children {
            kill_terminal(child);
        }
        runtimes::stop_runtimes(&self.root);
        let _ = fs::remove_dir_all(&self.root);
    }
}

// script owns the pty. Killing it closes the master, so the window's
// terminal hangs up while the window itself is not signalled.
fn kill_terminal(child: &mut Child) {
    // SAFETY: each child starts a private process group owned by this test.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.wait();
}

fn test_root(name: &str) -> PathBuf {
    let root = PathBuf::from(format!("/tmp/{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("test directory must be created");
    root
}

fn runtime_directory(root: &Path) -> PathBuf {
    root.join("state-home/seer/runtimes").join(USER)
}

fn socket_path(root: &Path) -> PathBuf {
    runtime_directory(root).join("socket")
}

fn wait_for_pid(directory: &Path) -> i32 {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Ok(text) = fs::read_to_string(directory.join("runtime.pid"))
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "the runtime PID must be recorded"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

// A window needs a real terminal, which is what starts the local runtime.
fn open_window(root: &Path) -> std::process::Child {
    window_command(root).spawn().expect("a window must start")
}

// nohup and the background shells of coding agents pass an ignored SIGHUP
// down to the window.
fn open_window_ignoring_hangup(root: &Path) -> std::process::Child {
    let mut command = window_command(root);
    // SAFETY: signal only changes a signal disposition before exec.
    unsafe {
        command.pre_exec(|| {
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            Ok(())
        });
    }
    command.spawn().expect("a window must start")
}

fn seer(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_seer"))
        .args(arguments)
        .env("XDG_CONFIG_HOME", root)
        .env("XDG_STATE_HOME", root.join("state-home"))
        .output()
        .expect("seer must run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn attached_windows(socket: &Path) -> Option<usize> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(WAIT)).ok()?;
    let ServerMsg::RuntimeReady { .. } = codec::decode(&mut stream).ok()? else {
        return None;
    };
    codec::encode(&mut stream, &ClientMsg::QueryStatus).ok()?;
    match codec::decode(&mut stream).ok()? {
        ServerMsg::Status { windows, .. } => Some(windows?.len()),
        _ => None,
    }
}

fn wait_for_windows(socket: &Path, count: usize, wait: Duration) {
    let deadline = Instant::now() + wait;
    while attached_windows(socket) != Some(count) {
        assert!(
            Instant::now() < deadline,
            "the runtime must report {count} attached windows"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

// Linux lists children per thread, and the runtime starts shells from a
// worker thread.
fn children_of(pid: i32) -> Vec<i32> {
    fs::read_dir(format!("/proc/{pid}/task"))
        .expect("the runtime's threads must be listed")
        .flatten()
        .filter_map(|task| fs::read_to_string(task.path().join("children")).ok())
        .flat_map(|children| {
            children
                .split_whitespace()
                .filter_map(|child| child.parse().ok())
                .collect::<Vec<i32>>()
        })
        .collect()
}

fn run_window_to_end(root: &Path) -> Output {
    let mut child = window_command(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("a window must start");
    let deadline = Instant::now() + WAIT;
    loop {
        if child
            .try_wait()
            .expect("window status must be available")
            .is_some()
        {
            return child
                .wait_with_output()
                .expect("window output must be read");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the window did not finish before the timeout");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn window_command(root: &Path) -> Command {
    let mut command = Command::new("script");
    command
        .process_group(0)
        .args([
            "-qec",
            &format!("{} attach", env!("CARGO_BIN_EXE_seer")),
            "/dev/null",
        ])
        .env("XDG_CONFIG_HOME", root)
        .env("XDG_STATE_HOME", root.join("state-home"))
        .env("TERM", "xterm")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn read_tree(socket: &Path) -> String {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Ok(mut stream) = UnixStream::connect(socket) {
            stream
                .set_read_timeout(Some(WAIT))
                .expect("read timeout must set");
            if let Ok(ServerMsg::RuntimeReady { .. }) = codec::decode::<_, ServerMsg>(&mut stream) {
                codec::encode(&mut stream, &ClientMsg::AttachRuntime).expect("attach must send");
                while let Ok(message) = codec::decode::<_, ServerMsg>(&mut stream) {
                    if let ServerMsg::Tree { tree } = message
                        && let Some(workspace) = tree.workspaces.first()
                        && !workspace.tabs.is_empty()
                    {
                        return format!("{:?}", workspace.tabs);
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("the local runtime never answered");
}

fn write_store(root: &Path) {
    write_store_for(root, "127.0.0.1:1");
}

fn write_store_for(root: &Path, endpoint: &str) {
    fs::create_dir_all(root.join("seer")).expect("config directory must be created");
    fs::write(
        root.join("seer/servers.toml"),
        format!(
            "[[servers]]\nendpoint = \"{endpoint}\"\nalias = \"room\"\nuser_id = \"{USER}\"\nname = \"alice\"\ncredential = \"secret\"\ncurrent = true\n"
        ),
    )
    .expect("server store must be written");
}

// Issue 338: a room that accepts the connection and never answers must not
// hold up the local terminals. A refused port fails at once and would hide it.
#[test]
fn a_room_that_never_answers_does_not_hold_up_this_computer() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a silent room must listen");
    let endpoint = listener.local_addr().expect("the room address").to_string();
    let held = thread::spawn(move || {
        let mut accepted = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            accepted.push(stream);
        }
    });

    let root = test_root("seer-silent");
    write_store_for(&root, &endpoint);

    let _windows = Windows {
        root: root.clone(),
        children: vec![open_window(&root)],
    };
    let start = Instant::now();
    read_tree(&socket_path(&root));
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "the local runtime must answer while the room stays silent"
    );

    wait_for_pid(&runtime_directory(&root));
    drop(held);
}

// Issue 429: a window that does not get SIGHUP, or ignores it, must still end
// when its terminal goes away. Its runtime stays for the next window.
#[test]
fn a_window_whose_terminal_goes_away_ends_and_leaves_its_runtime() {
    let root = test_root("seer-hangup");
    write_store(&root);
    let mut windows = Windows {
        root: root.clone(),
        children: vec![open_window_ignoring_hangup(&root)],
    };
    let socket = socket_path(&root);
    read_tree(&socket);
    let pid = wait_for_pid(&runtime_directory(&root));
    wait_for_windows(&socket, 1, WAIT);

    let mut terminal = windows.children.remove(0);
    kill_terminal(&mut terminal);

    wait_for_windows(&socket, 0, EXIT_WAIT);
    assert!(
        Path::new(&format!("/proc/{pid}")).exists(),
        "the runtime must outlive its window"
    );
    windows.children.push(open_window(&root));
    wait_for_windows(&socket, 1, WAIT);
}

// Issue 429: one command shows every Seer process of this person on this
// computer, and one command stops a named runtime with its shells.
#[test]
fn seer_ps_lists_the_processes_and_stops_a_named_runtime() {
    let root = test_root("seer-ps");
    write_store(&root);
    let _windows = Windows {
        root: root.clone(),
        children: vec![open_window(&root)],
    };
    let socket = socket_path(&root);
    read_tree(&socket);
    let pid = wait_for_pid(&runtime_directory(&root));
    wait_for_windows(&socket, 1, WAIT);
    let shells = children_of(pid);
    assert!(!shells.is_empty(), "the runtime must hold a shell");

    let listing = seer(&root, &["ps"]);
    let home = root.join("state-home").display().to_string();
    let runtime_row = listing
        .lines()
        .find(|line| line.starts_with(&format!("{pid} ")))
        .expect("the runtime must be listed");
    assert!(runtime_row.contains("runtime"), "{runtime_row}");
    assert!(runtime_row.contains("1 window, 1 shell"), "{runtime_row}");
    assert!(runtime_row.contains(&home), "{runtime_row}");
    assert!(
        listing.lines().any(|line| {
            line.contains("window") && line.contains("terminal pts/") && line.contains(&home)
        }),
        "the window must be listed with its terminal and state home:\n{listing}"
    );

    let stopped = seer(&root, &["ps", "--stop", &pid.to_string()]);
    assert!(
        stopped.contains(&format!("Runtime {pid} stopped with 1 shell")),
        "{stopped}"
    );
    let deadline = Instant::now() + EXIT_WAIT;
    while std::iter::once(&pid)
        .chain(&shells)
        .any(|process| Path::new(&format!("/proc/{process}")).exists())
    {
        assert!(
            Instant::now() < deadline,
            "the runtime and its shells must stop"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

// Issue 429: the command exists for wedged processes. A runtime with no
// socket that ignores SIGTERM must still be killed, and the success line
// must come only after it is gone.
#[test]
fn seer_ps_stop_kills_a_runtime_that_ignores_sigterm() {
    let root = test_root("seer-ps-kill");
    let _cleanup = CleanOnDrop(root.clone());
    let mut command = Command::new("sleep");
    command.arg0("seer-runtime").arg("1000");
    // SAFETY: signal only changes a signal disposition before exec.
    unsafe {
        command.pre_exec(|| {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
            Ok(())
        });
    }
    let mut child = command.spawn().expect("the fake runtime must start");
    let pid = child.id();
    let reaper = thread::spawn(move || child.wait());

    let stopped = seer(&root, &["ps", "--stop", &pid.to_string()]);

    assert!(
        stopped.contains(&format!("Runtime {pid} stopped")),
        "{stopped}"
    );
    let status = reaper
        .join()
        .expect("the reaper must finish")
        .expect("the fake runtime must be waited on");
    assert_eq!(status.signal(), Some(libc::SIGKILL));
}

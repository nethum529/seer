//! Forwarding and detach test helpers that are not part of the
//! identity test support module.
//!
//! These helpers are included only by the forwarding and detach
//! integration test crates. The identity test crate includes
//! forwarding/support.rs alone, so nothing here can live in that
//! shared module without causing dead code.

use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

use crate::support::{TestFiles, command_output, current_os_user, read_message, wait_for_file};

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(crate) fn write_config(files: &TestFiles, address: SocketAddr) {
    let user = current_os_user();
    files.write_config_with_os_users(address, &user, &user);
}

pub(crate) fn send(stream: &mut TcpStream, message: &ClientMsg) {
    codec::encode(stream, message).expect("client message must encode");
}

pub(crate) fn wait_for_cells(stream: &mut TcpStream) -> bool {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(read_message(stream), ServerMsg::Cells { .. }) {
            return true;
        }
    }
    false
}

pub(crate) fn pane_pid(files: &TestFiles, user: &str) -> u32 {
    let pid_file = files.root.join(format!("{user}-pane.pid"));
    assert!(wait_for_file(&pid_file));
    fs::read_to_string(pid_file)
        .expect("pane PID must be readable")
        .trim()
        .parse()
        .expect("pane PID must be valid")
}

pub(crate) fn assert_process_running(pid: u32) {
    let status =
        fs::read_to_string(format!("/proc/{pid}/status")).expect("process status must be readable");
    assert!(!status.lines().any(|line| line.starts_with("State:\tZ")));
}

pub(crate) fn assert_runtime_arguments(files: &TestFiles, user: &str) {
    let arguments_file = files.root.join(format!("{user}.args"));
    assert!(wait_for_file(&arguments_file));
    let arguments = fs::read_to_string(arguments_file).expect("runtime arguments must read");
    let expected_socket = files.xdg_runtime_dir.join(format!("seer/{user}.sock"));
    let expected_state = files.state_dir.join("users").join(user);
    let shell = current_login_shell();
    assert_eq!(
        arguments.lines().collect::<Vec<_>>(),
        [
            expected_socket.to_string_lossy().as_ref(),
            user,
            shell.as_str(),
            expected_state.to_string_lossy().as_ref()
        ]
    );
}

pub(crate) fn assert_socket_directory(files: &TestFiles) {
    let directory = files.xdg_runtime_dir.join("seer");
    let mode = fs::metadata(directory)
        .expect("socket directory metadata must load")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
}

pub(crate) fn assert_log_contains(files: &TestFiles, expected: &str) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if fs::read_to_string(&files.broker_log).is_ok_and(|contents| contents.contains(expected)) {
            return;
        }
        thread::sleep(POLL_INTERVAL);
    }
    panic!("broker log did not contain {expected}");
}

pub(crate) fn assert_log_excludes(files: &TestFiles, unexpected: &str) {
    let contents = fs::read_to_string(&files.broker_log).expect("broker log must read");
    assert!(!contents.contains(unexpected));
}

fn current_login_shell() -> String {
    let user = current_os_user();
    let record = command_output("getent", &["passwd", &user]);
    record
        .split(':')
        .nth(6)
        .expect("password record must contain a shell")
        .to_owned()
}

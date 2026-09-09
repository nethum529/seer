use std::process::Stdio;
use std::thread;
use std::time::Duration;

use seer_core::proto::ClientMsg;

use crate::support::*;

const GENERATION: &str = "0123456789abcdef0123456789abcdef";

// Issue 338: a runtime belongs to the person on their own computer. Whoever
// started it, and the room, can both go away while the shells keep running.
#[test]
fn the_runtime_and_its_shells_outlive_the_process_that_started_it() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let child = runtime_command()
        .args([
            socket_path.as_os_str(),
            "alice".as_ref(),
            "sh".as_ref(),
            GENERATION.as_ref(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let mut runtime = RuntimeProcess::new(child);

    let mut window = connect_when_ready(&socket_path);
    send(&mut window, &ClientMsg::AttachRuntime);
    let tree = read_until_tree(&mut window);
    let pane = tree.workspaces[0].tabs[0].panes[0].id.clone();

    runtime.close_parent_pipe();
    drop(window);
    thread::sleep(Duration::from_millis(300));

    assert!(
        runtime.is_running(),
        "the runtime must keep running when the process that started it goes away"
    );

    let mut reopened = connect_when_ready(&socket_path);
    send(&mut reopened, &ClientMsg::AttachRuntime);
    let same = read_until_tree(&mut reopened);
    assert_eq!(
        same.workspaces[0].tabs[0].panes[0].id, pane,
        "reopening a window must find the same shell, not a new one"
    );

    send_input(&mut reopened, &pane, "echo alive\n");
    assert!(
        wait_for_cells(&mut reopened),
        "typing must still reach the shell after the starter went away"
    );

    runtime.stop();
}

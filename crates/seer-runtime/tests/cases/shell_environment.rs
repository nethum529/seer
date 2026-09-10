use std::process::Stdio;

use crate::runtime_socket_helpers::*;
use crate::support::*;

const GENERATION: &str = "0123456789abcdef0123456789abcdef";
const CREDENTIAL: &str = "credential-sentinel-366";
const KEY: &str = "key-sentinel-366";

// Issue 366: a command in a Seer shell must not read the room secrets. The
// probe prints each name with the value the shell found in its environment.
#[test]
fn seer_shells_do_not_receive_the_room_secrets() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let runtime = runtime_command()
        .env("SEER_ROOM_CREDENTIAL", CREDENTIAL)
        .env("SEER_ROOM_KEY", KEY)
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
    let _runtime = RuntimeProcess::new(runtime);

    let mut stream = connect_with_timeout(&socket_path);
    let tree = read_until_tree(&mut stream);
    let workspace = &tree.workspaces[0];
    let tab = &workspace.tabs[0];
    let pane = tab.panes[0].id.clone();

    send_input_at(
        &mut stream,
        &workspace.id,
        &tab.id,
        &pane,
        "for n in CREDENTIAL KEY; do v=SEER_ROOM_$n; printf '%s=[%s]\\n' \"$v\" \"$(printenv $v)\"; done; for n in USER_ID PANE; do v=SEER_$n; printf '%s=[%s]\\n' \"$v\" \"$(printenv $v)\"; done\n",
    );
    let text = wait_for_cells_containing(&mut stream, &format!("SEER_PANE=[{pane}]"));

    assert!(text.contains("SEER_ROOM_CREDENTIAL=[]"), "{text}");
    assert!(text.contains("SEER_ROOM_KEY=[]"), "{text}");
    assert!(text.contains("SEER_USER_ID=[alice]"), "{text}");
    assert!(!text.contains(CREDENTIAL), "{text}");
    assert!(!text.contains(KEY), "{text}");
}

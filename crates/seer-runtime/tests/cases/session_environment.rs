use std::process::Stdio;

use crate::runtime_socket_helpers::*;
use crate::support::*;

const GENERATION: &str = "0123456789abcdef0123456789abcdef";

// Issue 402: the runtime keeps the environment of the terminal that started
// it. A Seer shell must not see the session markers of that terminal.
#[test]
fn seer_shells_do_not_inherit_the_starting_terminal_session() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let runtime = runtime_command()
        .env("HERDR_ENV", "1")
        .env("HERDR_PANE_ID", "w2:p6")
        .env("TMUX", "/tmp/tmux-1000/default,1,0")
        .env("TMUX_PANE", "%3")
        .env("STY", "1234.pts-0.host")
        .env("ZELLIJ", "0")
        .env("TERM_PROGRAM", "first-terminal")
        .env("SEER_KEEP_402", "kept")
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
        "for v in HERDR_ENV HERDR_PANE_ID TMUX TMUX_PANE STY ZELLIJ TERM_PROGRAM SEER_KEEP_402 HOME SEER_USER_ID SEER_PANE; do printf '%s=[%s]\\n' \"$v\" \"$(printenv $v)\"; done\n",
    );
    let text = wait_for_cells_containing(&mut stream, &format!("SEER_PANE=[{pane}]"));

    for name in [
        "HERDR_ENV",
        "HERDR_PANE_ID",
        "TMUX",
        "TMUX_PANE",
        "STY",
        "ZELLIJ",
        "TERM_PROGRAM",
    ] {
        assert!(text.contains(&format!("{name}=[]")), "{name}: {text}");
    }
    assert!(text.contains("SEER_KEEP_402=[kept]"), "{text}");
    assert!(!text.contains("HOME=[]"), "{text}");
    assert!(text.contains("SEER_USER_ID=[alice]"), "{text}");
}

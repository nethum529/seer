use std::fs;
use std::process::Stdio;

use crate::runtime_socket_helpers::*;
use crate::support::*;

const GENERATION: &str = "0123456789abcdef0123456789abcdef";
const ENDPOINT: &str = "endpoint-sentinel-501";
const CREDENTIAL: &str = "credential-sentinel-366";
const KEY: &str = "key-sentinel-366";

// Issues 366 and 501: the runtime settings are for the runtime only. A command
// in a Seer shell must not read the room secrets or the Seer state path.
#[test]
fn seer_shells_do_not_receive_the_runtime_settings() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let runtime = runtime_command()
        .env("SEER_SNAPSHOT_DIR", temporary.path.join("state"))
        .env("SEER_ROOM_ENDPOINT", ENDPOINT)
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
        "for n in SNAPSHOT_DIR ROOM_ENDPOINT ROOM_CREDENTIAL ROOM_KEY USER_ID PANE; do v=SEER_$n; printf '%s=[%s]\\n' \"$v\" \"$(printenv $v)\"; done\n",
    );
    let text = wait_for_cells_containing(&mut stream, &format!("SEER_PANE=[{pane}]"));

    for name in [
        "SEER_SNAPSHOT_DIR",
        "SEER_ROOM_ENDPOINT",
        "SEER_ROOM_CREDENTIAL",
        "SEER_ROOM_KEY",
    ] {
        assert!(text.contains(&format!("{name}=[]")), "{name}: {text}");
    }
    assert!(text.contains("SEER_USER_ID=[alice]"), "{text}");
}

// Issue 501: macOS /etc/zshrc and many user files set HISTFILE from ZDOTDIR.
// Seer points ZDOTDIR at its own files, so the history went into the Seer
// state directory instead of the file of a usual terminal.
#[test]
fn zsh_in_seer_keeps_the_history_file_of_a_usual_terminal() {
    let Some(text) = seer_zsh_screen(
        None,
        "HISTFILE=${ZDOTDIR:-$HOME}/.zsh_history\n",
        "print -r -- \"history=[${HISTFILE#$HOME/}] zdotdir=[${ZDOTDIR-unset}] probe-$((500 + 1))\"\n",
    ) else {
        return;
    };
    assert!(
        text.contains("history=[.zsh_history] zdotdir=[unset]"),
        "{text}"
    );
}

// Issue 501: with an empty ZDOTDIR, a plain zsh reads no startup files from
// HOME, so the functions and settings in them are not there.
#[test]
fn zsh_in_seer_keeps_an_empty_zdotdir_of_a_usual_terminal() {
    let Some(text) = seer_zsh_screen(
        Some(""),
        "SEER_PROBE_HOME=read\n",
        "print -r -- \"zdotdir=[${ZDOTDIR-unset}] home=[${SEER_PROBE_HOME-unset}] probe-$((500 + 1))\"\n",
    ) else {
        return;
    };
    assert!(text.contains("zdotdir=[] home=[unset]"), "{text}");
}

fn seer_zsh_screen(zdotdir: Option<&str>, zshenv: &str, line: &str) -> Option<String> {
    let Some(zsh) = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join("zsh"))
            .find(|candidate| candidate.is_file())
    }) else {
        eprintln!("zsh is not installed, skipped");
        return None;
    };
    let temporary = TemporaryDirectory::new();
    let home = temporary.path.join("home");
    fs::create_dir(&home).expect("home must be created");
    fs::write(home.join(".zshenv"), zshenv).expect("zshenv must be written");
    let socket_path = temporary.path.join("runtime.sock");
    let mut command = runtime_command();
    command
        .env("HOME", &home)
        .env("SEER_SNAPSHOT_DIR", temporary.path.join("state"))
        .env_remove("ZDOTDIR");
    if let Some(zdotdir) = zdotdir {
        command.env("ZDOTDIR", zdotdir);
    }
    let runtime = command
        .args([
            socket_path.as_os_str(),
            "alice".as_ref(),
            zsh.as_os_str(),
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
    send_input_at(&mut stream, &workspace.id, &tab.id, &tab.panes[0].id, line);
    Some(wait_for_cells_containing(&mut stream, "probe-501"))
}

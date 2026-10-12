use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{
    InputEvent, KeyCode, KeyInput, Modifiers, MouseButton, MouseInput, MouseKind, TerminalInput,
};

#[test]
fn shared_control_delivers_the_same_bytes_as_local_input() {
    let directory = PathBuf::from(format!("/tmp/s451-{}", std::process::id()));
    let (_runtime, mut owner, tree) = start_runtime(directory.clone(), "1;1003;1006;1004;2004");
    let workspace = &tree.workspaces[0];
    let tab = &workspace.tabs[0];
    let pane = &tab.panes[0].id;
    let events = [
        InputEvent::Text("Local UTF-8: \u{e9}".into()),
        InputEvent::Key(KeyInput {
            code: KeyCode::Up,
            modifiers: Modifiers::default(),
        }),
        InputEvent::Key(KeyInput {
            code: KeyCode::Function(5),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
        }),
        InputEvent::Paste("one\ntwo".into()),
        InputEvent::Mouse(MouseInput {
            kind: MouseKind::Down,
            button: Some(MouseButton::Left),
            column: 5,
            row: 2,
            modifiers: Modifiers::default(),
        }),
        InputEvent::Mouse(MouseInput {
            kind: MouseKind::Up,
            button: Some(MouseButton::Left),
            column: 5,
            row: 2,
            modifiers: Modifiers::default(),
        }),
        InputEvent::Focus(true),
        InputEvent::Focus(false),
    ];
    for event in &events {
        codec::encode(
            &mut owner,
            &ClientMsg::TerminalInput {
                workspace: workspace.id.clone(),
                tab: tab.id.clone(),
                pane: pane.clone(),
                input: TerminalInput::new(event.clone()),
            },
        )
        .unwrap();
    }
    let expected = b"Local UTF-8: \xc3\xa9\x1bOA\x1b[15;5~\x1b[200~one\ntwo\x1b[201~\x1b[<0;6;3M\x1b[<0;6;3m\x1b[I\x1b[O";
    wait_for_input(&directory, expected);
    let mut remote = connect(&directory, ClientMsg::ObserveRuntime);
    for event in events {
        let message = serde_json::json!({"GrantedTerminalInput": {
            "workspace": workspace.id, "tab": tab.id, "pane": pane,
            "input": TerminalInput::new(event), "sender": "Bob"
        }});
        codec::encode(&mut remote, &message).unwrap();
    }
    let twice = [expected.as_slice(), expected.as_slice()].concat();
    wait_for_input(&directory, &twice);
    codec::encode(
        &mut remote,
        &serde_json::json!({"GrantedTerminalInput": {
            "workspace": workspace.id, "tab": tab.id, "pane": pane,
            "input": TerminalInput::new(InputEvent::Scrollback { lines: 1 }), "sender": "Bob"
        }}),
    )
    .unwrap();
    loop {
        if matches!(
            codec::decode::<_, ServerMsg>(&mut remote).unwrap(),
            ServerMsg::Refused { .. }
        ) {
            break;
        }
    }
    loop {
        if let ServerMsg::Terminals { terminals, .. } = codec::decode(&mut owner).unwrap()
            && terminals
                .iter()
                .any(|terminal| terminal.last_typist.as_deref() == Some("Bob"))
        {
            break;
        }
    }
}

#[test]
fn shared_wheel_input_matches_local_arrows_without_mouse_reporting() {
    let directory = PathBuf::from(format!("/tmp/s451-wheel-{}", std::process::id()));
    let (_runtime, mut owner, tree) = start_runtime(directory.clone(), "1049");
    let workspace = &tree.workspaces[0];
    let tab = &workspace.tabs[0];
    let pane = &tab.panes[0].id;
    let mut remote = connect(&directory, ClientMsg::ObserveRuntime);
    for (shared, stream) in [(false, &mut owner), (true, &mut remote)] {
        for kind in [MouseKind::ScrollUp, MouseKind::ScrollDown] {
            let input = TerminalInput::new(InputEvent::Mouse(MouseInput {
                kind,
                button: None,
                column: 5,
                row: 2,
                modifiers: Modifiers::default(),
            }));
            let message = if !shared {
                ClientMsg::TerminalInput {
                    workspace: workspace.id.clone(),
                    tab: tab.id.clone(),
                    pane: pane.clone(),
                    input,
                }
            } else {
                ClientMsg::GrantedTerminalInput {
                    workspace: workspace.id.clone(),
                    tab: tab.id.clone(),
                    pane: pane.clone(),
                    input,
                    sender: "Bob".into(),
                }
            };
            codec::encode(&mut *stream, &message).unwrap();
        }
        let expected: &[u8] = if shared {
            b"\x1b[A\x1b[B\x1b[A\x1b[B"
        } else {
            b"\x1b[A\x1b[B"
        };
        wait_for_input(&directory, expected);
    }
}

fn start_runtime(directory: PathBuf, modes: &str) -> (Running, UnixStream, seer_core::Tree) {
    fs::create_dir_all(&directory).unwrap();
    let shell = directory.join("shell");
    fs::write(&shell, format!(
        "#!/bin/sh\nexec python3 -u -c 'import os,tty\ntty.setraw(0)\nf=open(\"{}/input\",\"ab\",buffering=0)\nos.write(1,b\"\\x1b[?{modes}hReady\")\nwhile True:\n b=os.read(0,4096)\n f.write(b)'\n",
        directory.display()
    )).unwrap();
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700)).unwrap();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    assert!(
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["build", "-j", "2", "-p", "seer", "--bin", "seer-runtime"])
            .status()
            .unwrap()
            .success()
    );
    let runtime = Running(
        Command::new(target.join("debug/seer-runtime"))
            .arg(directory.join("socket"))
            .arg("alice")
            .arg(shell)
            .arg("shared-input")
            .env_remove("SEER_SNAPSHOT_DIR")
            .env_remove("SEER_ROOM_ENDPOINT")
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
        directory.clone(),
    );
    let mut owner = connect(&directory, ClientMsg::AttachRuntime);
    let tree = loop {
        if let ServerMsg::Tree { tree } = codec::decode(&mut owner).unwrap() {
            break tree;
        }
    };
    loop {
        if let ServerMsg::Cells { frame, .. } = codec::decode(&mut owner).unwrap()
            && frame
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.character)
                .collect::<String>()
                .contains("Ready")
        {
            break;
        }
    }
    (runtime, owner, tree)
}

fn connect(directory: &std::path::Path, message: ClientMsg) -> UnixStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        if let Ok(stream) = UnixStream::connect(directory.join("socket")) {
            break stream;
        }
        assert!(Instant::now() < deadline, "runtime did not start");
        std::thread::sleep(Duration::from_millis(10));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    codec::encode(&mut stream, &message).unwrap();
    stream
}

fn wait_for_input(directory: &std::path::Path, expected: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let bytes = fs::read(directory.join("input")).unwrap_or_default();
        if bytes.len() >= expected.len() || Instant::now() >= deadline {
            assert_eq!(
                bytes, expected,
                "shared control must preserve terminal bytes"
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Running(Child, PathBuf);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.kill().ok();
        self.0.wait().ok();
        fs::remove_dir_all(&self.1).ok();
    }
}

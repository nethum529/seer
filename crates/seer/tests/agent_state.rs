use std::io;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, TerminalInput};

#[test]
fn agent_states_reach_owner_and_read_only_viewer() {
    let root = std::env::temp_dir().join(format!("seer-agent-state-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("test directory");
    let source = root.join("agent.rs");
    let capture = include_str!("fixtures/claude-short-spinner.txt");
    std::fs::write(
        &source,
        FIXTURE.replace("CAPTURE_LITERAL", &format!("{capture:?}")),
    )
    .expect("agent fixture");
    let fixture = root.join("claude");
    assert!(
        Command::new("rustc")
            .arg(&source)
            .arg("-o")
            .arg(&fixture)
            .status()
            .expect("build fixture")
            .success()
    );
    std::fs::copy(&fixture, root.join("codex")).expect("Codex fixture");
    std::fs::copy(&fixture, root.join("unknown")).expect("unknown fixture");
    let socket = root.join("socket");
    let mut runtime = Running(
        Command::new(env!("CARGO_BIN_EXE_seer-runtime"))
            .arg(&socket)
            .args(["alice", "sh", "agent-state-test"])
            .env_remove("SEER_SNAPSHOT_DIR")
            .env_remove("SEER_ROOM_ENDPOINT")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("runtime"),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut owner = loop {
        if let Ok(stream) = UnixStream::connect(&socket) {
            break stream;
        }
        assert!(Instant::now() < deadline, "runtime ready");
        std::thread::sleep(Duration::from_millis(10));
    };
    owner
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    codec::encode(&mut owner, &ClientMsg::AttachRuntime).expect("owner attach");
    let mut viewer = UnixStream::connect(&socket).expect("viewer");
    viewer
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    codec::encode(
        &mut viewer,
        &ClientMsg::Terminals {
            user: "alice".into(),
        },
    )
    .expect("catalog");
    wait_state(&mut owner, None);
    wait_state(&mut viewer, None);
    for agent in ["claude", "codex"] {
        input(&mut owner, &format!("{}\n", root.join(agent).display()));
        wait_state(&mut owner, Some("idle"));
        wait_state(&mut viewer, Some("idle"));
        for (key, state) in [
            ("short-work\n", "working"),
            ("short-completed\n", "idle"),
            ("work\n", "working"),
            ("short-transcript\n", "idle"),
            ("work\n", "working"),
            ("short-quoted\n", "idle"),
            ("work\n", "working"),
            ("permission\n", "waiting"),
            ("work\n", "working"),
            ("question\n", "waiting"),
            ("idle\n", "idle"),
            ("work\n", "working"),
            ("stale\n", "idle"),
            ("quiet-work\n", "working"),
        ] {
            input(&mut owner, key);
            check_both(&mut owner, &mut viewer, Some(state));
        }
        if agent == "claude" {
            for key in ["short-work\n", "short-title\n"] {
                input(&mut owner, key);
                std::thread::sleep(Duration::from_secs(1));
                for _ in 0..2 {
                    let mut stream = UnixStream::connect(&socket).expect("quiet viewer");
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .expect("timeout");
                    codec::encode(
                        &mut stream,
                        &ClientMsg::Terminals {
                            user: "alice".into(),
                        },
                    )
                    .expect("quiet catalog");
                    wait_state(&mut stream, Some("working"));
                }
            }
        }
        input(&mut owner, "exit\n");
        check_both(&mut owner, &mut viewer, None);
    }
    input(&mut owner, &format!("{}\n", root.join("unknown").display()));
    input(&mut owner, "permission\n");
    std::thread::sleep(Duration::from_millis(600));
    let mut fresh = UnixStream::connect(&socket).expect("fresh viewer");
    fresh
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    codec::encode(
        &mut fresh,
        &ClientMsg::Terminals {
            user: "alice".into(),
        },
    )
    .expect("catalog");
    wait_state(&mut fresh, None);
    input(&mut owner, "exit\n");
    drop(owner);
    runtime.0.kill().expect("stop runtime");
    runtime.0.wait().expect("wait runtime");
    std::fs::remove_dir_all(root).expect("remove directory");
}

fn input(stream: &mut UnixStream, text: &str) {
    codec::encode(
        stream,
        &ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p1".into(),
            input: TerminalInput::new(InputEvent::Text(text.into())),
        },
    )
    .expect("input");
}

fn check_both(owner: &mut UnixStream, viewer: &mut UnixStream, state: Option<&str>) {
    let start = Instant::now();
    wait_state(owner, state);
    wait_state(viewer, state);
    assert!(
        start.elapsed() < Duration::from_millis(1200),
        "state took {:?}",
        start.elapsed()
    );
}

fn wait_state(stream: &mut UnixStream, expected: Option<&str>) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        assert!(Instant::now() < deadline, "missing state {expected:?}");
        match codec::decode::<_, ServerMsg>(stream) {
            Ok(ServerMsg::Terminals { terminals, .. }) => {
                let Some(terminal) = terminals.first() else {
                    continue;
                };
                let value = serde_json::to_value(terminal).expect("metadata");
                if value["agent_state"].as_str() == expected {
                    return;
                }
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                panic!("missing state {expected:?}")
            }
            Err(error) => panic!("state read: {error}"),
        }
    }
}

struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const FIXTURE: &str = r#"
use std::io::{self, BufRead, Write};
const CAPTURE: &str = CAPTURE_LITERAL;
fn main() {
    let codex = std::env::args().next().unwrap().ends_with("codex");
    std::process::Command::new("stty").arg("-echo").status().unwrap();
    show(if codex { "› " } else { "────────────────────\n❯ \n────────────────────" });
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let text = match (codex, line.as_str()) {
            (_, "exit") => break,
            (false, "short-work") => CAPTURE,
            (false, "short-title") => { print!("\x1b]0;⠋ Claude\x07"); CAPTURE },
            (true, "short-work") => "• Working (2s • esc to interrupt)\n\n› ",
            (false, "short-completed") => "✻ Baked for 1s · done 11:34 PM\n────────────────────\n❯ \n────────────────────",
            (false, "short-transcript") => "· Gesticulating…\n⏺ Finished the task.\n────────────────────\n❯ \n────────────────────",
            (false, "short-quoted") => "⏺ Quoted output:\n```text\n· Gesticulating…\n```\n────────────────────\n❯ \n────────────────────",
            (false, "work" | "quiet-work") => "✻ Thinking… (2s · esc to interrupt)\n────────────────────\n❯ \n────────────────────",
            (true, "work" | "quiet-work") => "• Working (2s • esc to interrupt)\n\n› ",
            (false, "permission") => "────────────────────\nBash command\nDo you want to proceed?\n❯ 1. Yes\n  2. No\nEsc to cancel",
            (true, "permission") => "Would you like to run the following command?\n› 1. Yes, proceed (y)\n  2. No, and tell Codex what to do differently (esc)\nPress enter to confirm or esc to cancel",
            (false, "question") => "────────────────────\nWhich color?\n❯ 1. Blue\n  2. Red\nEnter to select · ↑/↓ to navigate · Esc to cancel",
            (true, "question") => "Which color?\n› 1. Blue\n  2. Red\nEnter to submit answer",
            (false, "stale") => "Old transcript: Do you want to proceed?\n1. Yes\nEsc to cancel\n────────────────────\n❯ \n────────────────────\n? for shortcuts",
            (true, "stale") => "Old transcript: Press enter to confirm or esc to cancel\n• Done.\n\n› \n? for shortcuts",
            (false, _) => "────────────────────\n❯ \n────────────────────\n? for shortcuts",
            (true, _) => "• Done.\n\n› \n? for shortcuts",
        };
        show(text);
    }
}
fn show(text: &str) { print!("\x1b[2J\x1b[H{}", text.replace('\n', "\r\n")); io::stdout().flush().unwrap(); }
"#;

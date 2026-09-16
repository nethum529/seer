use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Output};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::frame_diff::apply_next;
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, TerminalFrame, TerminalInput};

use super::support::*;

pub(crate) fn wait_for_socket_replacement(path: &Path, stale_inode: u64) {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match fs::metadata(path) {
            Ok(metadata) if metadata.ino() != stale_inode => return,
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => panic!("runtime socket metadata must load: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "runtime did not replace stale socket"
        );
        thread::sleep(RETRY_INTERVAL);
    }
}

pub(crate) fn connect_viewer(path: &Path) -> UnixStream {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match UnixStream::connect(path) {
            Ok(mut stream) => {
                codec::encode(
                    &mut stream,
                    &ClientMsg::Terminals {
                        user: "alice".into(),
                    },
                )
                .expect("viewer attach must encode");
                match codec::decode::<_, ServerMsg>(&mut stream).expect("runtime ready must decode")
                {
                    ServerMsg::RuntimeReady { generation, .. } => {
                        assert!(!generation.is_empty());
                        stream
                            .set_read_timeout(Some(MESSAGE_TIMEOUT))
                            .expect("read timeout must set");
                        return stream;
                    }
                    other => panic!("expected RuntimeReady, got {other:?}"),
                }
            }
            Err(error) => {
                assert!(Instant::now() < deadline, "viewer did not connect: {error}");
            }
        }
        thread::sleep(RETRY_INTERVAL);
    }
}

pub(crate) fn send_input_at(
    stream: &mut UnixStream,
    workspace: &str,
    tab: &str,
    pane: &str,
    input: &str,
) {
    send(
        stream,
        &ClientMsg::TerminalInput {
            workspace: workspace.into(),
            tab: tab.into(),
            pane: pane.into(),
            input: TerminalInput::new(InputEvent::Text(input.to_owned())),
        },
    );
}

pub(crate) fn read_until_tree_with_tab_count(
    stream: &mut UnixStream,
    tab_count: usize,
) -> seer_core::Tree {
    loop {
        let tree = read_until_tree(stream);
        if tree.workspaces[0].tabs.len() == tab_count {
            return tree;
        }
    }
}

pub(crate) fn wait_for_pane_cells_containing(stream: &mut UnixStream, pane: &str, expected: &str) {
    let start = Instant::now();
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "Cells did not contain {expected}"
        );
        if let ServerMsg::Cells {
            pane: message_pane,
            frame,
            ..
        } = read_message(stream)
            && message_pane == pane
            && frame_text(&frame).contains(expected)
        {
            return;
        }
    }
}

fn frame_text(frame: &TerminalFrame) -> String {
    frame
        .rows
        .iter()
        .flatten()
        .map(|cell| cell.character)
        .collect()
}

pub(crate) fn wait_for_refused(stream: &mut UnixStream) {
    while !matches!(read_message(stream), ServerMsg::Refused { .. }) {}
}

pub(crate) fn wait_for_bye(stream: &mut UnixStream) {
    while !matches!(read_message(stream), ServerMsg::Bye { .. }) {}
}

pub(crate) fn assert_no_message(stream: &mut UnixStream, pane: Option<&str>) {
    let deadline = Instant::now() + Duration::from_millis(250);
    stream
        .set_read_timeout(Some(Duration::from_millis(25)))
        .expect("short read timeout must set");
    loop {
        match codec::decode::<_, ServerMsg>(stream) {
            Ok(message) if pane.is_none() => {
                panic!("unexpected message after peek ended: {message:?}");
            }
            Ok(ServerMsg::Cells {
                pane: message_pane, ..
            })
            | Ok(ServerMsg::Frame {
                pane: message_pane, ..
            }) if pane.is_some_and(|expected| expected == message_pane) => {
                panic!("peek received an update for pane {message_pane}");
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    break;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(error) => panic!("unexpected message read failure: {error}"),
        }
    }
    stream
        .set_read_timeout(Some(MESSAGE_TIMEOUT))
        .expect("message read timeout must restore");
}

// Holds the screen as the client does (R-411): a whole screen replaces it,
// a diff at the next number applies, and any other diff asks for the whole
// screen again.
pub(crate) fn wait_for_cells_containing(stream: &mut UnixStream, expected: &str) -> String {
    let start = Instant::now();
    let mut held: Option<(TerminalFrame, u64)> = None;
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "Cells did not contain {expected}"
        );
        match read_message(stream) {
            ServerMsg::Cells { frame, seq, .. } => held = Some((frame, seq)),
            ServerMsg::CellsDiff {
                user,
                pane,
                seq,
                diff,
            } => {
                held = held
                    .and_then(|(frame, held_seq)| apply_next(&frame, held_seq, seq, &diff))
                    .map(|frame| (frame, seq));
                if held.is_none() {
                    send(stream, &ClientMsg::Resync { user, pane });
                }
            }
            _ => continue,
        }
        let text = held
            .iter()
            .flat_map(|(frame, _)| frame.rows.iter().flatten())
            .map(|cell| cell.character)
            .collect::<String>();
        if text.contains(expected) {
            return text;
        }
    }
}

pub(crate) fn wait_for_close(stream: &mut UnixStream) {
    let start = Instant::now();
    let mut bytes = [0; 1024];
    loop {
        assert!(
            start.elapsed() < MESSAGE_TIMEOUT,
            "connection did not close"
        );
        if stream.read(&mut bytes).expect("connection close must read") == 0 {
            return;
        }
    }
}

pub(crate) fn assert_usage_error(output: &Output) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("usage: seer-runtime <socket-path> <user> <shell> <generation>")
    );
}

pub(crate) fn wait_for_output(mut child: Child) -> Output {
    let start = Instant::now();
    loop {
        if child
            .try_wait()
            .expect("runtime status must be available")
            .is_some()
        {
            return child
                .wait_with_output()
                .expect("runtime output must be available");
        }
        if start.elapsed() >= PROCESS_TIMEOUT {
            let _ = child.kill();
            wait_until_exit(&mut child, "runtime did not stop after timeout");
            panic!("runtime did not exit within 2 seconds");
        }
        thread::sleep(RETRY_INTERVAL);
    }
}

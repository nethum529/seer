use std::io;
use std::os::unix::net::UnixStream;
use std::process::Stdio;
use std::time::{Duration, Instant};

use seer_core::proto::codec::{self, Incoming, RawEvent};
use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalFrame};
use seer_runtime::PaneGrid;

use crate::runtime_socket_helpers::*;
use crate::support::*;

const GENERATION: &str = "0123456789abcdef0123456789abcdef";

struct RawClient {
    stream: UnixStream,
    mirror: Option<PaneGrid>,
}

impl RawClient {
    fn read_until(&mut self, expected: &str) {
        let start = Instant::now();
        while !self
            .mirror
            .as_ref()
            .is_some_and(|grid| text(&grid.snapshot()).contains(expected))
        {
            assert!(
                start.elapsed() < MESSAGE_TIMEOUT,
                "raw stream did not show {expected}"
            );
            self.apply_next();
        }
        self.stream
            .set_read_timeout(Some(Duration::from_millis(300)))
            .expect("short read timeout must set");
        while self.apply_next() {}
        self.stream
            .set_read_timeout(Some(MESSAGE_TIMEOUT))
            .expect("read timeout must restore");
    }

    fn apply_next(&mut self) -> bool {
        let event = match codec::decode_incoming::<_, ServerMsg>(&mut self.stream) {
            Ok(Incoming::Raw(event)) => event,
            Ok(Incoming::Message(_)) => return true,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return false,
            Err(error) => panic!("raw stream must decode: {error}"),
        };
        match (event, &mut self.mirror) {
            (RawEvent::Start { cols, rows }, mirror) => {
                *mirror = Some(PaneGrid::mirror(cols, rows))
            }
            (RawEvent::Output(bytes), Some(grid)) => {
                grid.feed(&bytes);
            }
            (RawEvent::Size { cols, rows }, Some(grid)) => grid.resize(cols, rows),
            (RawEvent::SyncEnd, Some(grid)) => {
                grid.end_sync();
            }
            (event, None) => panic!("raw stream sent {event:?} before its start"),
        }
        true
    }

    fn snapshot(&self) -> TerminalFrame {
        self.mirror
            .as_ref()
            .expect("raw stream must start")
            .snapshot()
    }
}

fn text(frame: &TerminalFrame) -> String {
    frame
        .rows
        .iter()
        .flatten()
        .map(|cell| cell.character)
        .collect()
}

fn watch(pane: &str, cols: u16, rows: u16, viewer: bool) -> ClientMsg {
    ClientMsg::Watch {
        user: "alice".into(),
        pane: pane.into(),
        cols,
        rows,
        viewer,
    }
}

fn resize(client: &mut RawClient, pane: &str, cols: u16, rows: u16) {
    send(&mut client.stream, &watch(pane, cols, rows, true));
    send(
        &mut client.stream,
        &ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols,
            rows,
        },
    );
}

fn viewer_screen(socket: &std::path::Path, pane: &str, cols: u16, rows: u16) -> TerminalFrame {
    let mut viewer = connect_viewer(socket);
    send(&mut viewer, &watch(pane, cols, rows, false));
    loop {
        if let ServerMsg::Cells {
            pane: shown, frame, ..
        } = read_message(&mut viewer)
            && shown == pane
        {
            return frame;
        }
    }
}

#[test]
fn the_own_window_gets_raw_output_and_a_query_gets_one_answer() {
    let temporary = TemporaryDirectory::new();
    let socket = temporary.path.join("runtime.sock");
    let runtime = runtime_command()
        .args([
            socket.as_os_str(),
            "alice".as_ref(),
            "sh".as_ref(),
            GENERATION.as_ref(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let _runtime = RuntimeProcess::new(runtime);
    let mut stream = connect_with_timeout(&socket);
    send(
        &mut stream,
        &ClientMsg::TerminalCapabilities {
            capabilities: TerminalCapabilities {
                protocol_version: TERMINAL_PROTOCOL_VERSION,
                raw_output: true,
            },
        },
    );
    let pane = read_until_tree(&mut stream).workspaces[0].tabs[0].panes[0]
        .id
        .clone();
    let mut client = RawClient {
        stream,
        mirror: None,
    };
    resize(&mut client, &pane, 80, 24);

    send_input(
        &mut client.stream,
        &pane,
        "stty -echo -icanon min 0 time 10; printf '\\033[c'; r=$(head -c 64 | od -An -c | tr -d ' \\n'); stty sane; echo \"replies:$r:e\"\"nd\"\n",
    );
    client.read_until(":end");
    assert!(text(&client.snapshot()).contains("replies:033[?6c:end"));
    assert_eq!(viewer_screen(&socket, &pane, 80, 24), client.snapshot());

    resize(&mut client, &pane, 100, 30);
    send_input(&mut client.stream, &pane, "echo size:$(stty size):\n");
    client.read_until("size:30 100:");
    assert_eq!(viewer_screen(&socket, &pane, 100, 30), client.snapshot());
}

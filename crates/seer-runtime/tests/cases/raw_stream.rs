use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use seer_core::proto::codec::{self, Incoming, RawEvent};
use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{
    InputEvent, TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalFrame, TerminalInput,
};
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
            (RawEvent::Mark(mark), Some(_)) => send(&mut self.stream, &ClientMsg::Parsed { mark }),
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

fn start_runtime(socket: &Path) -> RuntimeProcess {
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
    RuntimeProcess::new(runtime)
}

fn open_window(socket: &Path) -> (RawClient, String) {
    let mut stream = connect_with_timeout(socket);
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
    (client, pane)
}

fn scrolled(client: &mut RawClient, lines: i32) -> TerminalFrame {
    let grid = client.mirror.as_mut().expect("raw stream must start");
    grid.handle_input(&TerminalInput::new(InputEvent::Scrollback { lines }))
        .expect("scrollback is valid input");
    grid.snapshot()
}

fn assert_same(open: &mut RawClient, reopened: &mut RawClient) {
    assert_eq!(reopened.snapshot(), open.snapshot());
    for lines in [1_000, -990, -10] {
        assert_eq!(scrolled(reopened, lines), scrolled(open, lines));
    }
}

#[test]
fn the_own_window_gets_raw_output_and_a_query_gets_one_answer() {
    let temporary = TemporaryDirectory::new();
    let socket = temporary.path.join("runtime.sock");
    let _runtime = start_runtime(&socket);
    let (mut client, pane) = open_window(&socket);

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

// Each stage stops the program in a state that a new window must rebuild:
// an open escape sequence (with a new line inside it) with the line drawing
// set active, the line drawing set selected, the shifted set with G0 and G1
// equal, and a full screen app over the shell.
#[test]
fn a_reopened_window_shows_what_a_window_that_stayed_open_shows() {
    let temporary = TemporaryDirectory::new();
    let socket = temporary.path.join("runtime.sock");
    let _runtime = start_runtime(&socket);
    let (mut open, pane) = open_window(&socket);

    send_input(
        &mut open.stream,
        &pane,
        concat!(
            "stty -echo; seq 40; ",
            "printf 'stage-%d\\033)0\\016\\033[31\\r\\n' 1; read x; ",
            "printf 'mlqqk\\017\\033[0m\\nstage-%d\\033(0' 2; read x; ",
            "printf 'lqqk\\033(B\\n\\033)B\\016stage-%d' 3; read x; ",
            "printf '\\033)0lqqk\\017\\nstage-%d' 4; read x; ",
            "printf '\\033[?1049h\\033[44mapp stage-%d' 5; read x; ",
            "printf '\\033[?1049l\\033[0mstage-%d\\n' 6; stty echo\n",
        ),
    );
    for stage in 1..6 {
        open.read_until(&format!("stage-{stage}"));
        let (mut reopened, _) = open_window(&socket);
        reopened.read_until(&format!("stage-{stage}"));
        send_input(&mut reopened.stream, &pane, "\n");
        let next = format!("stage-{}", stage + 1);
        open.read_until(&next);
        reopened.read_until(&next);
        assert_same(&mut open, &mut reopened);
    }
    assert!(text(&open.snapshot()).contains("\u{250c}\u{2500}\u{2500}\u{2510}"));
}

#[test]
fn output_keeps_flowing_while_the_program_does_not_read_its_input() {
    let temporary = TemporaryDirectory::new();
    let socket = temporary.path.join("runtime.sock");
    let _runtime = start_runtime(&socket);
    let (mut client, pane) = open_window(&socket);

    send_input(
        &mut client.stream,
        &pane,
        concat!(
            "stty raw -echo; echo re''ady; sleep 1; ",
            "head -c 1000000 /dev/zero | tr '\\0' x; head -c 100000 > /dev/null; ",
            "stty sane; echo; echo read-a''ll\n",
        ),
    );
    client.read_until("ready");
    send_input(&mut client.stream, &pane, &"y".repeat(100_000));
    client.read_until("read-all");
}

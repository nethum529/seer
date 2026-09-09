use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use seer_core::PaneSize;
use seer_core::proto::{ClientMsg, ServerMsg, codec};

use crate::support::*;

const GENERATION: &str = "0123456789abcdef0123456789abcdef";
const FIRST: PaneSize = PaneSize {
    cols: 190,
    rows: 50,
};
const REMOTE: PaneSize = PaneSize { cols: 80, rows: 24 };
const SECOND: PaneSize = PaneSize {
    cols: 153,
    rows: 40,
};
const SETTLE: Duration = Duration::from_millis(250);
const SIZE_TIMEOUT: Duration = Duration::from_secs(5);
const SIZE_POLL: Duration = Duration::from_millis(25);

#[test]
fn the_active_own_client_controls_the_pane_size() {
    let temporary = TemporaryDirectory::new();
    let socket_path = temporary.path.join("runtime.sock");
    let runtime = runtime_command()
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
    let mut runtime = RuntimeProcess::new(runtime);
    let mut first = connect_with_timeout(&socket_path);
    let tree = tree(read_message(&mut first));
    let pane = tree.workspaces[0].tabs[0].panes[0].id.clone();
    assert!(wait_for_cells(&mut first));
    let mut sizes = PaneSizes::new(&pane);

    send(&mut first, &watch(&pane, FIRST));
    sizes.settles_at(&mut first, FIRST);

    let mut remote = connect_observer(&socket_path);
    send(&mut remote, &watch(&pane, REMOTE));
    sizes.settles_at(&mut first, FIRST);

    send(
        &mut first,
        &ClientMsg::CreateTab {
            workspace: "w1".into(),
        },
    );
    let other = read_until_pane_count(&mut first, 2);

    let mut second = connect_with_timeout(&socket_path);
    read_until_tree(&mut second);
    send(&mut second, &watch(&pane, SECOND));
    sizes.settles_at(&mut first, SECOND);

    send(&mut first, &focus("w1:t1", &pane));
    sizes.settles_at(&mut first, FIRST);

    send(&mut second, &focus("w1:t2", &other));
    sizes.settles_at(&mut first, FIRST);

    send(&mut second, &watch(&pane, SECOND));
    sizes.settles_at(&mut first, FIRST);

    send(
        &mut second,
        &ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: SECOND.cols,
            rows: SECOND.rows,
        },
    );
    sizes.settles_at(&mut first, SECOND);

    send_input(&mut first, &pane, "");
    sizes.settles_at(&mut first, FIRST);

    drop(second);
    sizes.settles_at(&mut first, FIRST);

    send(&mut first, &unwatch(&pane));
    sizes.settles_at(&mut first, REMOTE);

    send(&mut remote, &unwatch(&pane));
    sizes.settles_at(&mut first, FIRST);

    send_input(&mut first, &pane, "stty size\n");
    wait_for_text(&mut first, "50 190");
    drop(first);
    assert!(runtime.stop().status.success());
}

fn watch(pane: &str, size: PaneSize) -> ClientMsg {
    ClientMsg::Watch {
        user: "alice".into(),
        pane: pane.into(),
        cols: size.cols,
        rows: size.rows,
    }
}

fn focus(tab: &str, pane: &str) -> ClientMsg {
    ClientMsg::FocusPane {
        workspace: "w1".into(),
        tab: tab.into(),
        pane: pane.into(),
    }
}

fn read_until_pane_count(stream: &mut UnixStream, panes: usize) -> String {
    let deadline = Instant::now() + MESSAGE_TIMEOUT;
    loop {
        let tree = read_until_tree(stream);
        let mut ids = tree
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.tabs)
            .flat_map(|tab| &tab.panes)
            .map(|pane| pane.id.clone());
        if ids.clone().count() == panes {
            return ids.next_back().expect("the new pane must exist");
        }
        assert!(
            Instant::now() < deadline,
            "tree never grew to {panes} panes"
        );
    }
}

fn unwatch(pane: &str) -> ClientMsg {
    ClientMsg::Unwatch {
        user: "alice".into(),
        pane: pane.into(),
    }
}

fn connect_observer(path: &Path) -> UnixStream {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        if let Ok(mut stream) = UnixStream::connect(path) {
            codec::encode(&mut stream, &ClientMsg::ObserveRuntime).expect("observe must encode");
            match codec::decode::<_, ServerMsg>(&mut stream).expect("runtime ready must decode") {
                ServerMsg::RuntimeReady { .. } => {
                    stream
                        .set_read_timeout(Some(MESSAGE_TIMEOUT))
                        .expect("read timeout must set");
                    return stream;
                }
                other => panic!("expected RuntimeReady, got {other:?}"),
            }
        }
        assert!(Instant::now() < deadline, "observer did not connect");
        thread::sleep(RETRY_INTERVAL);
    }
}

struct PaneSizes {
    pane: String,
    current: Option<PaneSize>,
}

impl PaneSizes {
    fn new(pane: &str) -> Self {
        Self {
            pane: pane.to_owned(),
            current: None,
        }
    }

    fn settles_at(&mut self, stream: &mut UnixStream, expected: PaneSize) {
        stream
            .set_read_timeout(Some(SIZE_POLL))
            .expect("short read timeout must set");
        let settle = Instant::now() + SETTLE;
        let deadline = Instant::now() + SIZE_TIMEOUT;
        loop {
            self.read_size(stream);
            let matched = self.current == Some(expected);
            if matched && Instant::now() >= settle {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "pane {} stayed at {:?}, expected {expected:?}",
                self.pane,
                self.current
            );
        }
        stream
            .set_read_timeout(Some(MESSAGE_TIMEOUT))
            .expect("message read timeout must restore");
    }

    fn read_size(&mut self, stream: &mut UnixStream) {
        match codec::decode::<_, ServerMsg>(stream) {
            Ok(ServerMsg::Terminals { terminals, .. }) => {
                if let Some(terminal) = terminals.iter().find(|terminal| terminal.pane == self.pane)
                {
                    self.current = Some(PaneSize {
                        cols: terminal.cols,
                        rows: terminal.rows,
                    });
                }
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(error) => panic!("runtime message must decode: {error}"),
        }
    }
}

fn wait_for_text(stream: &mut UnixStream, expected: &str) {
    let deadline = Instant::now() + MESSAGE_TIMEOUT;
    loop {
        if let ServerMsg::Cells { frame, .. } = read_message(stream) {
            let text: String = frame
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.character)
                .collect();
            if text.contains(expected) {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "cells never contained {expected}"
        );
    }
}

#![cfg(target_os = "linux")]

// Issue 418: these tests record what each receiver does today with a message
// from a different Seer version. docs/research/24-version-skew.md uses them.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{Cursor, PaneSize, TerminalFrame, TerminalModes, Tree};
use serde::Serialize;

#[path = "support/binary.rs"]
mod binary;
mod room;
use room::*;

const UNKNOWN_KIND: &str = r#"{"FutureKind":{"user":"alice"}}"#;

#[test]
fn a_runtime_ignores_unknown_fields_from_its_own_window_and_closes_it_on_other_changes() {
    let mut room = Room::start(false);
    let mut window = room.publish("alice", ALICE_SECRET);
    let workspace = own_tree(&mut window).workspaces[0].id.clone();

    send_raw(
        &mut window,
        r#"{"TerminalCapabilities":{"capabilities":{"protocol_version":1,"future":true}}}"#,
    );
    let create_tab = ClientMsg::CreateTab { workspace };
    send_raw(&mut window, &with_future_field(&create_tab, "CreateTab"));
    let reply = next_reply(
        &mut window,
        |message| matches!(message, ServerMsg::Tree { tree } if tree.workspaces[0].tabs.len() == 2),
    );
    assert!(
        matches!(reply, ServerMsg::Tree { .. }),
        "unknown fields must be ignored, got {reply:?}"
    );

    send_raw(&mut window, UNKNOWN_KIND);
    assert_closed(&mut window, "an unknown message kind");

    let mut window = own_window(&room.root.join("alice/socket"));
    send_raw(&mut window, r#"{"CreateTab":{}}"#);
    assert_closed(&mut window, "a missing field");
}

#[test]
fn a_runtime_ignores_unknown_fields_from_the_room_and_drops_the_link_on_other_changes() {
    let mut room = Room::start(false);
    let fake_broker = TcpListener::bind("127.0.0.1:0").expect("fake broker must bind");
    fake_broker
        .set_nonblocking(true)
        .expect("fake broker must not block");
    let endpoint = fake_broker
        .local_addr()
        .expect("fake broker must have an address")
        .to_string();
    let mut window = room.publish_to(&endpoint, "alice", ALICE_SECRET);
    let pane = own_tree(&mut window).workspaces[0].tabs[0].panes[0]
        .id
        .clone();
    let mut control = publication(&fake_broker);

    let open = ServerMsg::OpenStream { token: "t1".into() };
    send_raw(&mut control, &with_future_field(&open, "OpenStream"));
    let mut stream = runtime_stream(&fake_broker, "t1");
    let watch = ClientMsg::Watch {
        user: "alice".into(),
        pane: pane.clone(),
        cols: 80,
        rows: 24,
    };
    send_raw(&mut stream, &with_future_field(&watch, "Watch"));
    next_reply(
        &mut stream,
        |message| matches!(message, ServerMsg::Cells { pane: shown, .. } if *shown == pane),
    );
    send_raw(&mut stream, UNKNOWN_KIND);
    assert_closed(&mut stream, "an unknown message kind on a room stream");

    codec::encode(&mut control, &ServerMsg::OpenStream { token: "t2".into() })
        .expect("open request must send");
    let mut stream = runtime_stream(&fake_broker, "t2");
    send_raw(
        &mut stream,
        &format!(r#"{{"Watch":{{"user":"alice","pane":"{pane}"}}}}"#),
    );
    assert_closed(&mut stream, "a missing field on a room stream");

    send_raw(&mut control, UNKNOWN_KIND);
    assert_closed(&mut control, "an unknown message kind on the control link");
    let mut control = publication(&fake_broker);
    send_raw(&mut control, r#"{"OpenStream":{}}"#);
    assert_closed(&mut control, "a missing field on the control link");
    publication(&fake_broker);
}

#[test]
fn the_broker_ignores_unknown_fields_from_a_watcher_and_ends_its_link_on_other_changes() {
    let mut room = Room::start(false);
    let mut window = room.publish("alice", ALICE_SECRET);
    let pane = own_tree(&mut window).workspaces[0].tabs[0].panes[0]
        .id
        .clone();
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    wait_for_published(&mut alice, "alice");

    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    let watch = ClientMsg::Watch {
        user: "alice".into(),
        pane: pane.clone(),
        cols: 80,
        rows: 24,
    };
    send_raw(&mut bob, &with_future_field(&watch, "Watch"));
    wait_for(
        &mut bob,
        |message| matches!(message, ServerMsg::Cells { pane: shown, .. } if *shown == pane),
    );
    send_raw(&mut bob, UNKNOWN_KIND);
    assert_closed(&mut bob, "an unknown message kind");

    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    send_raw(
        &mut bob,
        &format!(r#"{{"Watch":{{"user":"alice","pane":"{pane}"}}}}"#),
    );
    assert_closed(&mut bob, "a missing field");
}

// The broker decodes each runtime message and encodes it again for the
// watcher, so a field that this broker does not know never reaches the watcher.
#[test]
fn the_broker_drops_unknown_fields_from_a_runtime_and_ends_its_stream_on_other_changes() {
    let room = Room::start(false);
    let mut control = TcpStream::connect(room.address).expect("fake runtime must connect");
    control
        .set_read_timeout(Some(WAIT))
        .expect("read timeout must set");
    send(
        &mut control,
        &ClientMsg::PublishRuntime {
            user_id: "bob".into(),
            credential: BOB_SECRET.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            generation: "gen-fake".into(),
        },
    );
    let ServerMsg::Published { .. } = decode(&mut control) else {
        panic!("the room must accept the fake runtime");
    };
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let (tree, pane) = one_pane_tree();

    let mut stream = watched_stream(room.address, &mut control, &mut alice, &tree, &pane);
    let cells = ServerMsg::Cells {
        user: "bob".into(),
        pane: pane.clone(),
        frame: TerminalFrame {
            rows: Vec::new(),
            cursor: Cursor::default(),
            modes: TerminalModes::default(),
        },
    };
    send_raw(&mut stream, &with_future_field(&cells, "Cells"));
    let forwarded = next_raw(&mut alice, "{\"Cells\"");
    assert!(
        !forwarded.contains("future"),
        "the broker must drop a field it does not know: {forwarded}"
    );

    send_raw(
        &mut stream,
        &format!(r#"{{"Cells":{{"user":"bob","pane":"{pane}"}}}}"#),
    );
    assert_closed(&mut stream, "a missing field");

    let mut stream = watched_stream(room.address, &mut control, &mut alice, &tree, &pane);
    send_raw(&mut stream, UNKNOWN_KIND);
    assert_closed(&mut stream, "an unknown message kind");

    send(&mut alice, &ClientMsg::ListPeople);
    wait_for(&mut alice, |message| {
        matches!(message, ServerMsg::People { .. })
    });
}

fn one_pane_tree() -> (Tree, String) {
    let mut tree = Tree::new();
    let workspace = tree.create_workspace("w").expect("workspace must open");
    let tab = tree
        .create_tab(&workspace.id, "t", PaneSize { cols: 80, rows: 24 })
        .expect("tab must open");
    let pane = tab.panes[0].id.clone();
    (tree, pane)
}

// The broker also opens streams for its status query, so skip every stream
// that does not observe.
fn watched_stream(
    address: SocketAddr,
    control: &mut TcpStream,
    watcher: &mut TcpStream,
    tree: &Tree,
    pane: &str,
) -> TcpStream {
    send(
        watcher,
        &ClientMsg::Watch {
            user: "bob".into(),
            pane: pane.into(),
            cols: 80,
            rows: 24,
        },
    );
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let ServerMsg::OpenStream { token } = decode(control) else {
            continue;
        };
        let mut stream = TcpStream::connect(address).expect("fake runtime stream must connect");
        stream
            .set_read_timeout(Some(WAIT))
            .expect("read timeout must set");
        send(
            &mut stream,
            &ClientMsg::RuntimeStream {
                user_id: "bob".into(),
                credential: BOB_SECRET.into(),
                token,
            },
        );
        if !matches!(codec::decode(&mut stream), Ok(ClientMsg::ObserveRuntime)) {
            continue;
        }
        for message in [
            ServerMsg::RuntimeReady {
                generation: "gen-fake".into(),
            },
            ServerMsg::Tree { tree: tree.clone() },
            ServerMsg::Terminals {
                user: "bob".into(),
                terminals: Vec::new(),
            },
        ] {
            codec::encode(&mut stream, &message).expect("fake runtime reply must send");
        }
        let watch: ClientMsg = codec::decode(&mut stream).expect("the watch must arrive");
        assert!(matches!(watch, ClientMsg::Watch { .. }));
        return stream;
    }
    panic!("the broker never asked the fake runtime to observe");
}

fn publication(fake_broker: &TcpListener) -> TcpStream {
    let mut control = accept(fake_broker);
    let ClientMsg::PublishRuntime { generation, .. } =
        codec::decode(&mut control).expect("publication must decode")
    else {
        panic!("a runtime must publish first");
    };
    codec::encode(&mut control, &ServerMsg::Published { generation })
        .expect("confirmation must send");
    control
}

fn runtime_stream(fake_broker: &TcpListener, expected: &str) -> TcpStream {
    let mut stream = accept(fake_broker);
    let message: ClientMsg = codec::decode(&mut stream).expect("stream request must decode");
    assert!(
        matches!(&message, ClientMsg::RuntimeStream { token, .. } if token == expected),
        "the runtime must open the requested stream, got {message:?}"
    );
    stream
}

fn accept(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Ok((stream, _)) = listener.accept() {
            stream.set_nonblocking(false).expect("stream must block");
            stream
                .set_read_timeout(Some(WAIT))
                .expect("read timeout must set");
            return stream;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("the runtime never connected to the fake broker");
}

fn with_future_field(message: &impl Serialize, kind: &str) -> String {
    let json = serde_json::to_string(message).expect("message must encode");
    let prefix = format!("{{\"{kind}\":{{");
    assert!(json.starts_with(&prefix), "{json} must be a {kind} message");
    json.replacen(&prefix, &format!("{prefix}\"future\":1,"), 1)
}

fn send_raw(stream: &mut impl Write, json: &str) {
    let length = u32::try_from(json.len()).expect("frame length must fit in u32");
    stream
        .write_all(&length.to_be_bytes())
        .and_then(|()| stream.write_all(json.as_bytes()))
        .expect("raw frame must send");
}

fn next_raw(stream: &mut impl Read, prefix: &str) -> String {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let mut length = [0; 4];
        stream
            .read_exact(&mut length)
            .expect("frame length must read");
        let mut body = vec![0; u32::from_be_bytes(length) as usize];
        stream.read_exact(&mut body).expect("frame body must read");
        let body = String::from_utf8(body).expect("frame must be JSON text");
        if body.starts_with(prefix) {
            return body;
        }
    }
    panic!("no frame started with {prefix}");
}

fn next_reply(stream: &mut impl Read, wanted: impl Fn(&ServerMsg) -> bool) -> ServerMsg {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let message = decode(stream);
        if wanted(&message) || matches!(message, ServerMsg::Refused { .. }) {
            return message;
        }
    }
    panic!("the expected reply never arrived");
}

fn assert_closed(stream: &mut impl Read, cause: &str) {
    let mut buffer = [0; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::ConnectionReset => return,
            Err(error) => panic!("{cause} must close the connection, got {error}"),
        }
    }
}

// Issue 418: what this window does with a message from a different Seer
// version. docs/research/24-version-skew.md uses these tests.

use std::io::Write;
use std::os::unix::net::UnixStream;

use super::*;
use seer_core::{Cursor, TerminalFrame, TerminalModes};

const PANE: &str = "w1:p1";
const UNKNOWN_KIND: &str = r#"{"FutureKind":{"user":"bob","pane":"w1:p1"}}"#;
const MISSING_FIELD: &str = r#"{"Cells":{"user":"bob","pane":"w1:p1"}}"#;

fn receive(source: Source, json: &str) -> (Option<SessionExit>, Routes, ClientState) {
    let (window, mut peer) = UnixStream::pair().expect("streams must open");
    let length = u32::try_from(json.len()).expect("frame length must fit in u32");
    peer.write_all(&length.to_be_bytes())
        .and_then(|()| peer.write_all(json.as_bytes()))
        .expect("raw frame must send");
    let (events, sender) = event_channel();
    let reader = spawn_reader(Socket::from(window), source, sender);
    let envelope = events.recv().expect("the reader must report the frame");
    drop(peer);
    drop(events);
    join_reader(reader).expect("the reader must stop");

    let (local, _) = UnixStream::pair().expect("streams must open");
    let (room, _) = UnixStream::pair().expect("streams must open");
    let mut routes = Routes::new(
        Socket::from(local),
        Some(Socket::from(room)),
        "alice".into(),
    );
    let mut state = ClientState::new(Tree::new(), "alice".into());
    let exit = drain(envelope, &mut routes, &mut state, &mut None).expect("drain must work");
    (exit, routes, state)
}

fn cells_with_unknown_field() -> String {
    let cells = ServerMsg::Cells {
        user: "bob".into(),
        pane: PANE.into(),
        frame: TerminalFrame {
            rows: Vec::new(),
            cursor: Cursor::default(),
            modes: TerminalModes::default(),
        },
        seq: 0,
    };
    let json = serde_json::to_string(&cells).expect("cells must encode");
    json.replacen(r#"{"Cells":{"#, r#"{"Cells":{"future":1,"#, 1)
}

#[test]
fn a_screen_update_with_an_unknown_field_shows_from_either_route() {
    for source in [Source::Local, Source::Room] {
        let (exit, _, state) = receive(source, &cells_with_unknown_field());
        assert_eq!(exit, None);
        assert!(
            state.frames.contains_key(&("bob".into(), PANE.into())),
            "{source:?} must show the frame"
        );
    }
}

#[test]
fn an_unreadable_message_from_the_own_runtime_ends_only_the_local_link() {
    for json in [UNKNOWN_KIND, MISSING_FIELD] {
        let (exit, ..) = receive(Source::Local, json);
        assert_eq!(exit, Some(SessionExit::LocalLinkLost), "{json}");
    }
}

#[test]
fn an_unreadable_message_from_the_room_drops_only_the_room() {
    for json in [UNKNOWN_KIND, MISSING_FIELD] {
        let (exit, mut routes, _) = receive(Source::Room, json);
        assert_eq!(exit, None, "{json}");
        assert!(routes.take_room_loss(), "{json} must mark the room offline");
    }
}

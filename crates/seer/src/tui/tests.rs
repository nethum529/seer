use super::*;
use seer_core::proto::codec;
use std::os::unix::net::UnixStream;

fn wires(own_user: &str) -> (Routes, UnixStream, UnixStream) {
    let (local, local_peer) = UnixStream::pair().expect("streams must open");
    let (room, room_peer) = UnixStream::pair().expect("streams must open");
    let routes = Routes::new(
        Socket::from(local),
        Some(Socket::from(room)),
        own_user.to_owned(),
    );
    (routes, local_peer, room_peer)
}

fn closed() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "closed")
}

#[test]
fn the_room_stopping_leaves_the_window_and_the_local_terminals() {
    let (mut routes, _local, _room) = wires("alice");
    let mut state = ClientState::new(Tree::new(), "alice".into());

    let exit = drain(
        Envelope {
            source: Source::Room,
            message: Ok(ServerMsg::Bye {
                reason: "server stopped".into(),
            }),
        },
        &mut routes,
        &mut state,
        &mut None,
    )
    .expect("a room goodbye must be handled");

    assert_eq!(
        exit, None,
        "the room stopping must not end this computer's window"
    );
    assert!(routes.take_room_loss(), "the room must be marked offline");
}

#[test]
fn detaching_this_client_still_ends_the_window() {
    let (mut routes, _local, _room) = wires("alice");
    let mut state = ClientState::new(Tree::new(), "alice".into());

    let exit = drain(
        Envelope {
            source: Source::Room,
            message: Ok(ServerMsg::Bye {
                reason: "detached".into(),
            }),
        },
        &mut routes,
        &mut state,
        &mut None,
    )
    .expect("a detach must be handled");

    assert_eq!(exit, Some(SessionExit::Detached));
}

#[test]
fn losing_the_local_runtime_ends_the_window() {
    let (mut routes, _local, _room) = wires("alice");
    let mut state = ClientState::new(Tree::new(), "alice".into());

    let exit = drain(
        Envelope {
            source: Source::Local,
            message: Err(closed()),
        },
        &mut routes,
        &mut state,
        &mut None,
    )
    .expect("a local failure must be handled");

    assert_eq!(exit, Some(SessionExit::ServerStopped));
}

#[test]
fn losing_the_room_keeps_the_window_and_the_local_terminals() {
    let (mut routes, mut local, _room) = wires("alice");
    let mut state = ClientState::new(Tree::new(), "alice".into());
    state.terminals.insert("alice".into(), Vec::new());
    state.terminals.insert("bob".into(), Vec::new());

    let exit = drain(
        Envelope {
            source: Source::Room,
            message: Err(closed()),
        },
        &mut routes,
        &mut state,
        &mut None,
    )
    .expect("a room failure must be handled");
    assert_eq!(exit, None, "the window must stay open without the room");
    assert!(routes.take_room_loss(), "the room loss must be reported");
    note_room_offline(&mut state);
    assert!(state.terminals.contains_key("alice"));
    assert!(
        !state.terminals.contains_key("bob"),
        "another person's terminals must be marked unavailable"
    );

    crate::tui::send(
        &mut routes,
        &ClientMsg::CreateTab {
            workspace: "w".into(),
        },
    )
    .expect("local work must continue without the room");
    assert!(
        codec::decode::<_, ClientMsg>(&mut local).is_ok(),
        "the local route must still carry own terminal work"
    );
}

#[test]
fn room_input_is_dropped_while_the_room_is_gone() {
    let (mut routes, mut local, _room) = wires("alice");
    routes.drop_room();

    crate::tui::send(
        &mut routes,
        &ClientMsg::TypeInto {
            user: "bob".into(),
            pane: "p".into(),
            bytes: b"x".to_vec(),
        },
    )
    .expect("remote input must be dropped, not queued");

    local
        .set_read_timeout(Some(Duration::from_millis(50)))
        .expect("timeout must apply");
    let mut byte = [0];
    assert!(
        std::io::Read::read(&mut local, &mut byte).is_err(),
        "remote input must never reach the local runtime"
    );
}

fn screen(text: &str) -> seer_core::TerminalFrame {
    let cell = |character| seer_core::Cell {
        character,
        fg: seer_core::Color::Default,
        bg: seer_core::Color::Default,
        bold: false,
        italic: false,
        underline: false,
        dim: false,
        inverse: false,
        hidden: false,
        strikeout: false,
    };
    seer_core::TerminalFrame {
        rows: vec![text.chars().map(cell).collect()],
        cursor: seer_core::Cursor::default(),
        modes: seer_core::TerminalModes::default(),
    }
}

fn from_room(routes: &mut Routes, state: &mut ClientState, message: ServerMsg) {
    drain(
        Envelope {
            source: Source::Room,
            message: Ok(message),
        },
        routes,
        state,
        &mut None,
    )
    .expect("a room screen message must be handled");
}

fn diff_message(from: &str, to: &str, seq: u64) -> ServerMsg {
    ServerMsg::CellsDiff {
        user: "bob".into(),
        pane: "p".into(),
        seq,
        diff: seer_core::frame_diff::diff(&screen(from), &screen(to)).expect("same shape"),
    }
}

fn cells_message(text: &str, seq: u64) -> ServerMsg {
    ServerMsg::Cells {
        user: "bob".into(),
        pane: "p".into(),
        frame: screen(text),
        seq,
    }
}

fn no_message(peer: &mut UnixStream) -> bool {
    let mut byte = [0];
    std::io::Read::read(peer, &mut byte).is_err()
}

// R-411: a diff that does not follow the held screen leaves it as it is
// and asks the room once for the whole screen. The next whole screen puts
// the viewer back in step, and the diff after it applies.
#[test]
fn a_diff_out_of_step_asks_once_and_the_next_whole_screen_recovers() {
    let (mut routes, _local, mut room) = wires("alice");
    room.set_read_timeout(Some(Duration::from_millis(50)))
        .expect("timeout must apply");
    let mut state = ClientState::new(Tree::new(), "alice".into());
    let key = ("bob".to_owned(), "p".to_owned());
    state
        .watches
        .insert(key.clone(), (ratatui::layout::Size::new(2, 1), true));

    from_room(&mut routes, &mut state, cells_message("ab", 5));
    from_room(&mut routes, &mut state, diff_message("ab", "xb", 7));
    assert_eq!(
        state.frames[&key],
        screen("ab"),
        "a diff out of step must not apply"
    );
    assert_eq!(
        codec::decode::<_, ClientMsg>(&mut room).expect("the viewer must ask again"),
        ClientMsg::Resync {
            user: "bob".into(),
            pane: "p".into()
        }
    );

    from_room(&mut routes, &mut state, diff_message("ab", "xb", 8));
    assert_eq!(state.frames[&key], screen("ab"));
    assert!(
        no_message(&mut room),
        "the viewer asks once until the whole screen comes"
    );

    from_room(&mut routes, &mut state, cells_message("cd", 9));
    assert_eq!(
        state.frames[&key],
        screen("cd"),
        "a whole screen always replaces the held one"
    );
    from_room(&mut routes, &mut state, diff_message("cd", "ce", 10));
    assert_eq!(
        state.frames[&key],
        screen("ce"),
        "the diff after the whole screen applies"
    );
    assert!(no_message(&mut room));
}

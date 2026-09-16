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

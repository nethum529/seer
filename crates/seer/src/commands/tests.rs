use std::io::{self, Cursor};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientInfo, Person, ServerMsg, codec};
use seer_core::{Cell, Cursor as TerminalCursor, TerminalFrame, TerminalModes, Tree};

use super::{
    edit_distance_at_most_one, finish_session, is_close, people_reply, pick_client, pick_server,
    receive_clients, receive_reply_before, welcome_tree,
};
use crate::store::ServerEntry;
use crate::tui::SessionExit;

#[test]
fn picker_retries_and_selects_a_number() {
    let servers = [server("one"), server("two")];
    let mut output = Vec::new();
    let selected = pick_server(&servers, &mut Cursor::new(b"2\n"), &mut output)
        .expect("picker must select a server");

    assert_eq!(selected.alias, "two");
    assert_eq!(
        String::from_utf8(output).expect("picker output must be UTF-8"),
        "Select a server:\n  1. one (alice)\n  2. two (alice)\nServer: "
    );
}

#[test]
fn close_names_use_prefix_or_one_edit() {
    assert!(is_close("ali", "Alice"));
    assert!(is_close("alice", "alixe"));
    assert!(is_close("alice", "alice1"));
    assert!(!is_close("alice", "bob"));
    assert!(edit_distance_at_most_one(b"abc", b"abc"));
    assert!(edit_distance_at_most_one(b"abc", b"ac"));
    assert!(edit_distance_at_most_one(b"ab", b"abc"));
    assert!(edit_distance_at_most_one(b"abc", b"ab"));
    assert!(edit_distance_at_most_one(b"ab", b"zab"));
    assert!(edit_distance_at_most_one(b"zab", b"ab"));
    assert!(!edit_distance_at_most_one(b"abc", b"ayx"));
    assert!(!edit_distance_at_most_one(b"abc", b"axyd"));
}

#[test]
fn picker_reports_end_of_input() {
    let error = pick_server(
        &[server("one"), server("two")],
        &mut Cursor::new([]),
        &mut Vec::new(),
    )
    .expect_err("empty input must fail");
    assert_eq!(error.code, 1);
    assert_eq!(error.message, "no server selected");
}

#[test]
fn client_picker_selects_a_number_and_rejects_empty_input() {
    let clients = [client("first", 5), client("second", 10)];
    let mut output = Vec::new();
    let selected = pick_client(&clients, &mut Cursor::new(b"2\n"), &mut output)
        .expect("picker must select a client");

    assert_eq!(selected, clients[1]);
    assert_eq!(
        String::from_utf8(output).expect("picker output must be UTF-8"),
        "Select a client:\n  1. first (connected 5 seconds)\n  2. second (connected 10 seconds)\nClient: "
    );
    let error = pick_client(&clients, &mut Cursor::new([]), &mut Vec::new())
        .expect_err("empty input must fail");
    assert_eq!(error.code, 1);
    assert_eq!(error.message, "no client selected");
}

#[test]
fn handshake_reply_helpers_reject_refused_and_unexpected_messages() {
    let refused = ServerMsg::Refused {
        reason: "not allowed".into(),
    };
    assert_eq!(
        people_reply(refused.clone())
            .expect_err("refusal must fail")
            .message,
        "not allowed"
    );
    assert_eq!(
        welcome_tree(refused)
            .expect_err("refusal must fail")
            .message,
        "refused: not allowed"
    );
    let unexpected = ServerMsg::People {
        people: vec![Person {
            user_id: "user-1".into(),
            name: "alice".into(),
            attached_clients: 0,
            peekable: true,
        }],
    };
    assert_eq!(
        welcome_tree(unexpected)
            .expect_err("unexpected reply must fail")
            .message,
        "error: unexpected server reply"
    );
    assert_eq!(
        people_reply(ServerMsg::Tree { tree: Tree::new() })
            .expect_err("unexpected reply must fail")
            .message,
        "error: unexpected server reply"
    );
}

#[test]
fn client_reply_skips_runtime_messages_and_reports_refusal() {
    let (mut client_stream, mut server_stream) = socket_pair();
    codec::encode(&mut server_stream, &ServerMsg::Tree { tree: Tree::new() })
        .expect("Tree must encode");
    codec::encode(
        &mut server_stream,
        &ServerMsg::Clients {
            clients: vec![client("client-1", 2)],
        },
    )
    .expect("Clients must encode");
    assert_eq!(
        receive_clients(&mut client_stream).expect("Clients must be received"),
        vec![client("client-1", 2)]
    );

    let (mut client_stream, mut server_stream) = socket_pair();
    codec::encode(
        &mut server_stream,
        &ServerMsg::Refused {
            reason: "not allowed".into(),
        },
    )
    .expect("Refused must encode");
    let error = receive_clients(&mut client_stream).expect_err("Refused must fail");
    assert_eq!(error.code, 1);
    assert_eq!(error.message, "not allowed");
}

#[test]
fn terminal_session_configures_peek_and_runs() {
    let (client, _server) = socket_pair();
    let result = finish_session(
        true,
        client.into(),
        Tree::new(),
        Some("alice"),
        "team",
        |_, tree| {
            assert!(tree.workspaces.is_empty());
            Ok(SessionExit::Client)
        },
    );
    assert!(result.is_ok());

    let (client, _server) = socket_pair();
    let error = finish_session(true, client.into(), Tree::new(), None, "team", |_, _| {
        Err(io::Error::other("TUI failed"))
    })
    .expect_err("TUI failure must propagate");
    assert_eq!(error.message, "error: TUI failed");

    let (client, _server) = socket_pair();
    let result = finish_session(false, client.into(), Tree::new(), None, "team", |_, _| {
        Err(io::Error::other("runner must not be called"))
    });
    assert!(result.is_ok());
}

#[test]
fn command_reply_skips_all_stream_messages() {
    let (mut client, mut server) = socket_pair();
    let writer = thread::spawn(move || {
        for message in [
            ServerMsg::Tree { tree: Tree::new() },
            ServerMsg::Frame {
                pane: "p1".into(),
                bytes: vec![1],
            },
            ServerMsg::Cells {
                pane: "p1".into(),
                frame: TerminalFrame {
                    rows: vec![Vec::<Cell>::new()],
                    cursor: TerminalCursor::default(),
                    modes: TerminalModes::default(),
                    scrollback_offset: 0,
                },
            },
            ServerMsg::Seat {
                capsule: "seat".into(),
                expires_in_secs: 1,
            },
        ] {
            codec::encode(&mut server, &message).expect("message must encode");
        }
    });

    let reply = receive_reply_before(&mut client, Instant::now() + Duration::from_secs(1))
        .expect("command reply must decode");

    assert!(matches!(reply, ServerMsg::Seat { .. }));
    writer.join().expect("writer must finish");
}

#[test]
fn command_reply_has_one_deadline_for_all_messages() {
    let (mut client, mut server) = socket_pair();
    let writer = thread::spawn(move || {
        codec::encode(&mut server, &ServerMsg::Tree { tree: Tree::new() })
            .expect("Tree must encode");
        thread::sleep(Duration::from_millis(100));
    });
    let started = Instant::now();

    let error = receive_reply_before(&mut client, started + Duration::from_millis(20))
        .expect_err("silent command reply must time out");

    assert_eq!(error.code, 2);
    assert!(started.elapsed() < Duration::from_millis(90));
    writer.join().expect("writer must finish");
}

fn socket_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let client = TcpStream::connect(listener.local_addr().expect("address must exist"))
        .expect("client must connect");
    let (server, _) = listener.accept().expect("server must accept");
    (client, server)
}

fn server(alias: &str) -> ServerEntry {
    ServerEntry {
        endpoint: format!("{alias}:7321"),
        alias: alias.into(),
        user_id: "user-1".into(),
        name: "alice".into(),
        credential: "secret".into(),
        current: false,
    }
}

fn client(client_id: &str, connected_secs: u64) -> ClientInfo {
    ClientInfo {
        client_id: client_id.into(),
        connected_secs,
    }
}

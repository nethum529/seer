use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientInfo, ServerMsg, codec};
use seer_core::{Cell, Cursor as TerminalCursor, TerminalFrame, TerminalModes, Tree};

use super::{edit_distance_at_most_one, is_close, receive_clients, receive_reply_before};

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
fn command_reply_skips_all_stream_messages() {
    let (mut command_stream, mut server) = socket_pair();
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

    let reply = receive_reply_before(&mut command_stream, Instant::now() + Duration::from_secs(1))
        .expect("command reply must decode");

    assert!(matches!(reply, ServerMsg::Seat { .. }));
    writer.join().expect("writer must finish");

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

fn client(client_id: &str, connected_secs: u64) -> ClientInfo {
    ClientInfo {
        client_id: client_id.into(),
        connected_secs,
    }
}

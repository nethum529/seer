use std::io::{self, Cursor};
use std::net::{TcpListener, TcpStream};

use seer_core::Tree;
use seer_core::proto::{Person, ServerMsg};

use super::{
    edit_distance_at_most_one, finish_session, is_close, people_reply, pick_server, welcome_tree,
};
use crate::store::ServerEntry;

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
fn terminal_session_configures_peek_and_runs() {
    let (client, _server) = socket_pair();
    let result = finish_session(true, client, Tree::new(), Some("alice"), |_, tree| {
        assert!(tree.workspaces.is_empty());
        Ok(())
    });
    assert!(result.is_ok());

    let (client, _server) = socket_pair();
    let error = finish_session(true, client, Tree::new(), None, |_, _| {
        Err(io::Error::other("TUI failed"))
    })
    .expect_err("TUI failure must propagate");
    assert_eq!(error.message, "error: TUI failed");

    let (client, _server) = socket_pair();
    let result = finish_session(false, client, Tree::new(), None, |_, _| {
        Err(io::Error::other("runner must not be called"))
    });
    assert!(result.is_ok());
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

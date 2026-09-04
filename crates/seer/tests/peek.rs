use std::fs;
use std::net::{TcpListener, TcpStream};
use std::thread;

use seer_core::Tree;
use seer_core::proto::{ClientMsg, PeekTarget, ServerMsg};

#[path = "support/cli.rs"]
mod cli_support;
#[path = "support/server_io.rs"]
mod server_io;

use cli_support::{TestConfig, accept, listener, run, text};
use server_io::receive;

#[test]
fn peek_resolves_an_active_target_and_handles_selection_failures() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    write_store(&config, address.port());
    let server = thread::spawn(move || {
        let mut close = accept(&listener);
        prepare_people(&mut close);

        let mut active = prepare_query(&listener);
        send(
            &mut active,
            &ServerMsg::Targets {
                targets: targets(true),
            },
        );
        assert_peek(&mut active, "w4", "w4:t9");
        send(&mut active, &ServerMsg::Tree { tree: Tree::new() });

        let mut picker = prepare_query(&listener);
        send(
            &mut picker,
            &ServerMsg::Targets {
                targets: targets(false),
            },
        );
        assert_peek(&mut picker, "w4", "w4:t9");
        send(&mut picker, &ServerMsg::Tree { tree: Tree::new() });

        let mut empty = prepare_query(&listener);
        send(
            &mut empty,
            &ServerMsg::Targets {
                targets: Vec::new(),
            },
        );

        let mut stale = prepare_query(&listener);
        send(
            &mut stale,
            &ServerMsg::Targets {
                targets: vec![target("w9", "w9:t7", "old", true)],
            },
        );
        assert_peek(&mut stale, "w9", "w9:t7");
        send(
            &mut stale,
            &ServerMsg::Refused {
                reason: "tab not found".into(),
            },
        );
    });

    let close = run(&config, &["peek", "alic"], "");
    assert_eq!(close.status.code(), Some(1));
    assert!(close.stdout.is_empty());
    assert_eq!(close.stderr, b"Close names: alice\nno person named alic\n");

    let active = run(&config, &["peek", "alice"], "");
    assert_eq!(active.status.code(), Some(0));
    assert_eq!(
        active.stdout,
        b"PEEK: alice - READ ONLY\nWorkspace: alice/w4\n"
    );
    assert!(active.stderr.is_empty());

    let picker = run(&config, &["peek", "alice"], "2\n");
    assert_eq!(picker.status.code(), Some(0));
    assert_eq!(
        text(&picker.stdout),
        "Select a target:\n  1. main/shell\n  2. work/tests\nTarget: PEEK: alice - READ ONLY\nWorkspace: alice/w4\n"
    );
    assert!(picker.stderr.is_empty());

    let empty = run(&config, &["peek", "alice"], "");
    assert_eq!(empty.status.code(), Some(1));
    assert!(empty.stdout.is_empty());
    assert_eq!(empty.stderr, b"no active target\n");

    let stale = run(&config, &["peek", "alice"], "");
    assert_eq!(stale.status.code(), Some(1));
    assert!(stale.stdout.is_empty());
    assert_eq!(stale.stderr, b"tab not found\n");
    server.join().expect("server must finish");
}

fn prepare_people(stream: &mut TcpStream) {
    assert_hello(stream);
    send_welcome(stream, "user-bob", "bob");
    assert_eq!(receive(stream), ClientMsg::ListPeople);
    send(
        stream,
        &ServerMsg::People {
            people: vec![person("user-alice", "alice", 1)],
        },
    );
}

fn prepare_query(listener: &TcpListener) -> TcpStream {
    let mut stream = accept(listener);
    prepare_people(&mut stream);
    assert_eq!(
        receive(&mut stream),
        ClientMsg::QueryTargets {
            user: "user-alice".into(),
        }
    );
    stream
}

fn assert_peek(stream: &mut TcpStream, workspace: &str, tab: &str) {
    assert_eq!(
        receive(stream),
        ClientMsg::Peek {
            user: "user-alice".into(),
            workspace: workspace.into(),
            tab: tab.into(),
        }
    );
}

fn targets(second_active: bool) -> Vec<PeekTarget> {
    vec![
        target("w1", "w1:t8", "main", false),
        target("w4", "w4:t9", "work", second_active),
    ]
}

fn target(workspace: &str, tab: &str, name: &str, active: bool) -> PeekTarget {
    PeekTarget {
        workspace: workspace.into(),
        workspace_name: name.into(),
        tab: tab.into(),
        tab_title: if workspace == "w4" {
            "tests".into()
        } else {
            "shell".into()
        },
        active,
    }
}

fn assert_hello(stream: &mut TcpStream) {
    assert_eq!(
        receive(stream),
        ClientMsg::Hello {
            user_id: "user-bob".into(),
            credential: "device-secret".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    );
}

fn send_welcome(stream: &mut TcpStream, user_id: &str, name: &str) {
    send(
        stream,
        &ServerMsg::Welcome {
            user_id: user_id.into(),
            name: name.into(),
            client_id: "client-1".into(),
            tree: Tree::new(),
        },
    );
}

fn person(user_id: &str, name: &str, attached_clients: u32) -> seer_core::proto::Person {
    seer_core::proto::Person {
        user_id: user_id.into(),
        name: name.into(),
        attached_clients,
        peekable: true,
    }
}

fn send(stream: &mut TcpStream, message: &ServerMsg) {
    seer_core::proto::codec::encode(stream, message).expect("server message must encode");
}

fn write_store(config: &TestConfig, port: u16) {
    let directory = config.root.join("seer");
    fs::create_dir_all(&directory).expect("config directory must exist");
    fs::write(
        directory.join("servers.toml"),
        format!(
            "[[servers]]\nendpoint = \"127.0.0.1:{port}\"\nalias = \"team.example.com\"\nuser_id = \"user-bob\"\nname = \"bob\"\ncredential = \"device-secret\"\ncurrent = true\n"
        ),
    )
    .expect("store must be written");
}

use std::os::unix::net::UnixStream;
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{InputEvent, TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalInput};

use super::SharedSession;
use crate::UserSession;

#[test]
fn identifies_only_mutating_messages() {
    let mutating = [
        ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "p1".into(),
            input: TerminalInput::new(InputEvent::Text(String::new())),
        },
        ClientMsg::CreateTab {
            workspace: "w1".into(),
        },
        ClientMsg::SplitPane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            direction: seer_core::SplitDirection::Right,
        },
        ClientMsg::ClosePane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "p1".into(),
        },
        ClientMsg::FocusPane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "p1".into(),
        },
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 80,
            rows: 24,
        },
    ];
    let deferred = [
        ClientMsg::Hello {
            user_id: "alice".into(),
            credential: "token".into(),
            version: "0.1.0".into(),
        },
        ClientMsg::Join {
            seat_token: "seat".into(),
            name: "alice".into(),
        },
        ClientMsg::Invite { hours: None },
        ClientMsg::ListPeople,
        ClientMsg::DetachClient {
            client_id: "client-1".into(),
        },
        ClientMsg::AttachRuntime,
        ClientMsg::QueryTargets {
            user: "alice".into(),
        },
        ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
            tab: "w1:t1".into(),
        },
        ClientMsg::StopPeek,
        ClientMsg::Detach,
        ClientMsg::TerminalCapabilities {
            capabilities: TerminalCapabilities {
                protocol_version: TERMINAL_PROTOCOL_VERSION,
            },
        },
    ];

    assert!(mutating.iter().all(ClientMsg::is_mutating));
    assert!(deferred.iter().all(|message| !message.is_mutating()));
}

#[test]
fn polls_output_without_connections() {
    let mut session = UserSession::new("alice", "sh");
    session
        .ensure_first_shell()
        .expect("first shell must start");
    let shared = SharedSession::new(session);
    shared
        .apply_and_broadcast(ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p1".into(),
            input: TerminalInput::new(InputEvent::Text("printf detached-output\\n".into())),
        })
        .expect("input must succeed");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut updated = false;
    while Instant::now() < deadline {
        let (messages, has_connections) = shared
            .poll_and_broadcast()
            .expect("session poll must succeed");
        assert!(!has_connections);
        updated = messages.iter().any(|message| match message {
            ServerMsg::Cells { frame, .. } => frame
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.character)
                .collect::<String>()
                .contains("detached-output"),
            _ => false,
        });
        if updated {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(updated);
    shared
        .apply_and_broadcast(ClientMsg::ClosePane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p1".into(),
        })
        .expect("pane close must succeed");
}

#[test]
fn evicts_stalled_connection_without_blocking_other_client() {
    let shared = SharedSession::new(UserSession::new("alice", "sh"));
    let (stalled_server, _stalled_client) =
        UnixStream::pair().expect("stalled stream pair must open");
    let (healthy_server, mut healthy_client) =
        UnixStream::pair().expect("healthy stream pair must open");
    healthy_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout must set");
    shared
        .add_connection(1, stalled_server)
        .expect("stalled connection must be added");
    shared
        .add_connection(2, healthy_server)
        .expect("healthy connection must be added");
    let _: seer_core::proto::ServerMsg =
        seer_core::proto::codec::decode(&mut healthy_client).expect("initial tree must decode");

    let reader = thread::spawn(move || {
        loop {
            let message: seer_core::proto::ServerMsg =
                seer_core::proto::codec::decode(&mut healthy_client)
                    .expect("healthy output must decode");
            if matches!(message, seer_core::proto::ServerMsg::Frame { pane, .. } if pane == "recovered")
            {
                return;
            }
        }
    });
    let large = seer_core::proto::ServerMsg::Frame {
        pane: "fill".into(),
        bytes: vec![b'x'; 256 * 1024],
    };
    for _ in 0..68 {
        shared
            .broadcast(std::slice::from_ref(&large))
            .expect("large output must broadcast");
        if shared.connection_count() == 1 {
            break;
        }
    }

    assert_eq!(shared.connection_count(), 1);
    shared
        .broadcast(&[seer_core::proto::ServerMsg::Frame {
            pane: "recovered".into(),
            bytes: Vec::new(),
        }])
        .expect("recovery output must broadcast");
    reader.join().expect("healthy reader must finish");
}

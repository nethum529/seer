use std::os::unix::net::UnixStream;
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{PaneSize, Tree};

use super::{SIZE_LEASE_TIMEOUT, SharedSession, handle_message, is_mutating, lock};
use crate::UserSession;

#[test]
fn identifies_only_mutating_messages() {
    let mutating = [
        ClientMsg::Input {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "p1".into(),
            bytes: Vec::new(),
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
        ClientMsg::Peek {
            user: "alice".into(),
            workspace: "w1".into(),
            tab: "w1:t1".into(),
        },
        ClientMsg::StopPeek,
        ClientMsg::Detach,
    ];

    assert!(mutating.iter().all(is_mutating));
    assert!(deferred.iter().all(|message| !is_mutating(message)));
}

#[test]
fn removes_only_the_requested_connection() {
    let mut session = UserSession::new("alice", "sh");
    session
        .ensure_first_shell()
        .expect("first shell must start");
    session
        .apply(ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 1,
            rows: 1,
        })
        .expect("session must resize");
    let shared = SharedSession::new(session);
    let (first_server, _first_client) = UnixStream::pair().expect("stream pair must open");
    let (second_server, _second_client) = UnixStream::pair().expect("stream pair must open");
    shared
        .add_connection(1, first_server)
        .expect("first connection must be added");
    shared
        .add_connection(2, second_server)
        .expect("second connection must be added");

    shared
        .remove_connection(1)
        .expect("connection must be removed");
    let connections = lock(&shared.connections).expect("connections must lock");
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].id, 2);
}

#[test]
fn polls_output_without_connections() {
    let mut session = UserSession::new("alice", "sh");
    session
        .ensure_first_shell()
        .expect("first shell must start");
    let shared = SharedSession::new(session);
    shared
        .apply_and_broadcast(ClientMsg::Input {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p1".into(),
            bytes: b"printf detached-output\\n".to_vec(),
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
            ServerMsg::Cells { rows, .. } => rows
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

#[test]
fn size_lease_governs_per_client_resize_control() {
    let shared = SharedSession::new(UserSession::new("alice", "sh"));
    let (owner_server, mut owner_client) = UnixStream::pair().expect("stream pair must open");
    let (peer_server, mut peer_client) = UnixStream::pair().expect("stream pair must open");
    let (peek_server, mut peek_client) = UnixStream::pair().expect("stream pair must open");

    // The first full attachment acquires the size lease.
    shared
        .add_connection(1, owner_server)
        .expect("owner must attach");
    assert_eq!(owner_id(&shared), Some(1));
    assert_eq!(
        pane_size_of(&read_until_tree(&mut owner_client)),
        PaneSize { cols: 80, rows: 24 }
    );

    // A second full attachment does not take the lease.
    shared
        .add_connection(2, peer_server)
        .expect("peer must attach");
    assert_eq!(owner_id(&shared), Some(1));
    assert_eq!(
        pane_size_of(&read_until_tree(&mut peer_client)),
        PaneSize { cols: 80, rows: 24 }
    );

    // A resize from the owner applies to the shared geometry.
    handle_message(
        &shared,
        1,
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 120,
            rows: 40,
        },
    )
    .expect("owner resize must apply");
    assert_eq!(
        pane_size_of(&read_until_tree(&mut owner_client)),
        PaneSize {
            cols: 120,
            rows: 40
        }
    );
    assert_eq!(
        pane_size_of(&read_until_tree(&mut peer_client)),
        PaneSize {
            cols: 120,
            rows: 40
        }
    );
    assert_eq!(
        session_pane_size(&shared),
        PaneSize {
            cols: 120,
            rows: 40
        }
    );

    // A resize from the peer is denied, but its own viewport is recorded.
    handle_message(
        &shared,
        2,
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 100,
            rows: 30,
        },
    )
    .expect("denied resize must not fail");
    assert_eq!(owner_id(&shared), Some(1));
    assert_eq!(
        viewport_of(&shared, 2),
        Some(PaneSize {
            cols: 100,
            rows: 30
        })
    );
    assert_eq!(
        viewport_of(&shared, 1),
        Some(PaneSize {
            cols: 120,
            rows: 40
        })
    );
    assert_eq!(
        session_pane_size(&shared),
        PaneSize {
            cols: 120,
            rows: 40
        }
    );

    // A peek attachment cannot resize the shared geometry.
    shared
        .add_connection(3, peek_server)
        .expect("peek viewer must attach");
    assert_eq!(
        pane_size_of(&read_until_tree(&mut peek_client)),
        PaneSize {
            cols: 120,
            rows: 40
        }
    );
    handle_message(
        &shared,
        3,
        ClientMsg::Peek {
            user: "bob".into(),
            workspace: "w1".into(),
            tab: "w1:t1".into(),
        },
    )
    .expect("peek must start");
    assert!(read_only_of(&shared, 3));
    assert_eq!(
        pane_size_of(&read_until_tree(&mut peek_client)),
        PaneSize {
            cols: 120,
            rows: 40
        }
    );
    handle_message(
        &shared,
        3,
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 200,
            rows: 50,
        },
    )
    .expect("peek resize must be dropped");
    assert_eq!(viewport_of(&shared, 3), None);
    assert_eq!(owner_id(&shared), Some(1));
    assert_eq!(
        session_pane_size(&shared),
        PaneSize {
            cols: 120,
            rows: 40
        }
    );

    // A peek disconnect does not move the lease.
    shared
        .remove_connection(3)
        .expect("peek viewer must detach");
    assert_eq!(owner_id(&shared), Some(1));

    // An owner silent past the timeout loses the lease to the resizing peer.
    {
        let mut connections = lock(&shared.connections).expect("connections must lock");
        let owner = connections
            .iter_mut()
            .find(|connection| connection.id == 1)
            .expect("owner must exist");
        owner.last_active = Instant::now() - SIZE_LEASE_TIMEOUT - Duration::from_secs(1);
    }
    handle_message(
        &shared,
        2,
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 90,
            rows: 30,
        },
    )
    .expect("takeover resize must apply");
    assert_eq!(owner_id(&shared), Some(2));
    assert_eq!(session_pane_size(&shared), PaneSize { cols: 90, rows: 30 });
    assert_eq!(
        pane_size_of(&read_until_tree(&mut owner_client)),
        PaneSize { cols: 90, rows: 30 }
    );
    assert_eq!(
        pane_size_of(&read_until_tree(&mut peer_client)),
        PaneSize { cols: 90, rows: 30 }
    );

    // The former owner is denied while the new owner stays active.
    handle_message(
        &shared,
        1,
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 110,
            rows: 40,
        },
    )
    .expect("denied resize must not fail");
    assert_eq!(owner_id(&shared), Some(2));
    assert_eq!(
        viewport_of(&shared, 1),
        Some(PaneSize {
            cols: 110,
            rows: 40
        })
    );
    assert_eq!(session_pane_size(&shared), PaneSize { cols: 90, rows: 30 });

    // Owner disconnect releases the lease to the surviving attachment,
    // which adopts its recorded viewport and broadcasts the new geometry.
    shared.remove_connection(2).expect("owner must detach");
    assert_eq!(owner_id(&shared), Some(1));
    assert_eq!(
        pane_size_of(&read_until_tree(&mut owner_client)),
        PaneSize {
            cols: 110,
            rows: 40
        }
    );
    assert_eq!(
        session_pane_size(&shared),
        PaneSize {
            cols: 110,
            rows: 40
        }
    );

    // The surviving attachment resizes normally.
    handle_message(
        &shared,
        1,
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 130,
            rows: 45,
        },
    )
    .expect("survivor resize must apply");
    assert_eq!(owner_id(&shared), Some(1));
    assert_eq!(
        session_pane_size(&shared),
        PaneSize {
            cols: 130,
            rows: 45
        }
    );
    assert_eq!(
        pane_size_of(&read_until_tree(&mut owner_client)),
        PaneSize {
            cols: 130,
            rows: 45
        }
    );
}

fn owner_id(shared: &SharedSession) -> Option<u64> {
    let connections = lock(&shared.connections).expect("connections must lock");
    connections
        .iter()
        .find(|connection| connection.size_owner)
        .map(|connection| connection.id)
}

fn viewport_of(shared: &SharedSession, id: u64) -> Option<PaneSize> {
    let connections = lock(&shared.connections).expect("connections must lock");
    connections
        .iter()
        .find(|connection| connection.id == id)
        .and_then(|connection| connection.viewport.as_ref())
        .map(|viewport| PaneSize {
            cols: viewport.cols,
            rows: viewport.rows,
        })
}

fn read_only_of(shared: &SharedSession, id: u64) -> bool {
    let connections = lock(&shared.connections).expect("connections must lock");
    connections
        .iter()
        .find(|connection| connection.id == id)
        .is_some_and(|connection| connection.read_only)
}

fn session_pane_size(shared: &SharedSession) -> PaneSize {
    let session = lock(&shared.session).expect("session must lock");
    pane_size_of(&session.tree)
}

fn read_until_tree(stream: &mut UnixStream) -> Tree {
    loop {
        if let ServerMsg::Tree { tree } = codec::decode(stream).expect("server message must decode")
        {
            return tree;
        }
    }
}

fn pane_size_of(tree: &Tree) -> PaneSize {
    tree.workspaces[0].tabs[0].panes[0].size
}

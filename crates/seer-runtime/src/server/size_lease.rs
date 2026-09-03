use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{PaneSize, Tree};

use super::{SIZE_LEASE_TIMEOUT, SharedSession, handle_message, lock};
use crate::UserSession;

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

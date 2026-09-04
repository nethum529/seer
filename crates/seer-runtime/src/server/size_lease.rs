use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{PaneSize, TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, Tree};

use super::{SIZE_LEASE_TIMEOUT, SharedSession, handle_message, lock};
use crate::UserSession;

#[test]
fn size_lease_governs_per_client_resize_control() {
    let shared = SharedSession::new(UserSession::new("alice", "sh"));
    let (owner_server, mut owner_client) = UnixStream::pair().expect("stream pair must open");
    let (peer_server, mut peer_client) = UnixStream::pair().expect("stream pair must open");

    shared
        .add_connection(1, owner_server)
        .expect("owner must attach");
    assert_eq!(
        pane_size_of(&read_until_tree(&mut owner_client)),
        PaneSize { cols: 80, rows: 24 }
    );

    shared
        .add_connection(2, peer_server)
        .expect("peer must attach");
    assert_eq!(
        pane_size_of(&read_until_tree(&mut peer_client)),
        PaneSize { cols: 80, rows: 24 }
    );

    let capabilities = TerminalCapabilities {
        protocol_version: TERMINAL_PROTOCOL_VERSION,
    };
    handle_message(&shared, 1, ClientMsg::TerminalCapabilities { capabilities })
        .expect("valid capabilities must be accepted");

    handle_message(
        &shared,
        2,
        ClientMsg::TerminalCapabilities {
            capabilities: TerminalCapabilities {
                protocol_version: TERMINAL_PROTOCOL_VERSION + 1,
            },
        },
    )
    .expect("invalid capabilities must be refused");
    read_until_refused(&mut peer_client);

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
    assert_eq!(
        pane_size_of(&read_until_tree(&mut owner_client)),
        PaneSize { cols: 90, rows: 30 }
    );
    assert_eq!(
        pane_size_of(&read_until_tree(&mut peer_client)),
        PaneSize { cols: 90, rows: 30 }
    );

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

    shared.remove_connection(2).expect("owner must detach");
    assert_eq!(
        pane_size_of(&read_until_tree(&mut owner_client)),
        PaneSize {
            cols: 110,
            rows: 40
        }
    );

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
    assert_eq!(
        pane_size_of(&read_until_tree(&mut owner_client)),
        PaneSize {
            cols: 130,
            rows: 45
        }
    );
}

fn read_until_tree(stream: &mut UnixStream) -> Tree {
    loop {
        if let ServerMsg::Tree { tree } = codec::decode(stream).expect("server message must decode")
        {
            return tree;
        }
    }
}

fn read_until_refused(stream: &mut UnixStream) {
    loop {
        if matches!(
            codec::decode(stream).expect("server message must decode"),
            ServerMsg::Refused { .. }
        ) {
            return;
        }
    }
}

fn pane_size_of(tree: &Tree) -> PaneSize {
    tree.workspaces[0].tabs[0].panes[0].size
}

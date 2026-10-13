use super::revocation::{pending_viewer, publish_delayed};
use super::*;
use seer_core::{Cursor, LayoutNode, PaneSize, TerminalFrame, TerminalModes, Tree};
use std::collections::BTreeSet;
use std::io::{self, Read};
use std::net::{Shutdown, TcpStream};

#[test]
fn reconnect_restores_large_watch_sets_and_closes_failed_observers() {
    let room = Room::start(false);
    let mut publisher = publish_delayed(&room);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    let (tree, panes) = watched_tree();
    watch(&mut bob, &panes[0]);
    let mut target = ready_target(&room, &mut publisher, &tree);
    received_watch(&mut target, &panes[0]);
    for pane in &panes[1..] {
        watch(&mut bob, pane);
        received_watch(&mut target, pane);
    }
    client_barrier(&mut bob);
    target.shutdown(Shutdown::Write).unwrap();
    target.set_read_timeout(Some(WAIT)).unwrap();
    target.read_to_end(&mut Vec::new()).unwrap();

    watch(&mut bob, &panes[0]);
    let mut failed = ready_target(&room, &mut publisher, &tree);
    wait_for(&mut bob, |m| matches!(m, ServerMsg::Refused { .. }));
    failed
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let result = failed.read_to_end(&mut Vec::new());
    assert!(
        result.is_ok() || result.is_err_and(|e| e.kind() == io::ErrorKind::ConnectionReset),
        "a failed reconnect must close its observer stream"
    );
    client_barrier(&mut bob);

    watch(&mut bob, &panes[0]);
    let mut resumed = ready_target(&room, &mut publisher, &tree);
    std::thread::sleep(Duration::from_millis(100));
    resumed.set_read_timeout(Some(WAIT)).unwrap();
    resumed.set_write_timeout(Some(WAIT)).unwrap();
    std::thread::scope(|scope| {
        let shown = scope.spawn(|| {
            let mut shown = BTreeSet::new();
            while shown.len() < panes.len() {
                match decode(&mut bob) {
                    ServerMsg::Cells {
                        user, pane, seq: 1, ..
                    } => {
                        assert_eq!(user, "alice");
                        shown.insert(pane);
                    }
                    ServerMsg::Refused { reason } => panic!("restore refused: {reason}"),
                    _ => {}
                }
            }
            assert!(shown == panes.iter().cloned().collect());
        });
        let mut restored = BTreeSet::new();
        for _ in 0..=panes.len() {
            let message: ClientMsg = codec::decode(&mut resumed)
                .expect("all saved watches must return after the target resumes");
            let ClientMsg::Watch {
                user,
                pane,
                cols,
                rows,
                viewer,
            } = message
            else {
                panic!("the runtime must receive a watch");
            };
            assert_eq!(user, "alice");
            assert_eq!((cols, rows, viewer), (80, 24, true));
            restored.insert(pane.clone());
            codec::encode(
                &mut resumed,
                &ServerMsg::Cells {
                    user: "alice".into(),
                    pane,
                    frame: TerminalFrame {
                        rows: Vec::new(),
                        cursor: Cursor::default(),
                        modes: TerminalModes::default(),
                    },
                    seq: 1,
                },
            )
            .expect("each restored watch must accept its screen reply");
        }
        assert!(restored == panes.iter().cloned().collect());
        shown.join().unwrap();
    });
    client_barrier(&mut bob);
}

fn watched_tree() -> (Tree, Vec<String>) {
    let mut tree = Tree::new();
    let workspace = tree.create_workspace("Shared").unwrap();
    let mut panes = Vec::new();
    for index in 0..256 {
        tree.create_tab(&workspace.id, "Shell", PaneSize { cols: 80, rows: 24 })
            .unwrap();
        let tab = tree.workspaces[0].tabs.last_mut().unwrap();
        // Long valid IDs make the target pause exceed the transport buffer.
        let pane = format!("{index:03}-{}", "p".repeat(24 * 1024));
        tab.panes[0].id.clone_from(&pane);
        tab.layout.root = Some(LayoutNode::Pane { pane: pane.clone() });
        tab.layout.focused = None;
        panes.push(pane);
    }
    (tree, panes)
}

fn ready_target(room: &Room, publisher: &mut TcpStream, tree: &Tree) -> TcpStream {
    let mut target = pending_viewer(publisher, room.address);
    for message in [
        ServerMsg::RuntimeReady {
            generation: "delayed".into(),
            version: None,
            room_refused: None,
        },
        ServerMsg::Tree { tree: tree.clone() },
        ServerMsg::Terminals {
            user: "alice".into(),
            terminals: Vec::new(),
        },
    ] {
        codec::encode(&mut target, &message).unwrap();
    }
    target
}

fn watch(client: &mut TcpStream, pane: &str) {
    send(
        client,
        &ClientMsg::Watch {
            user: "alice".into(),
            pane: pane.into(),
            cols: 80,
            rows: 24,
            viewer: true,
        },
    );
}

fn received_watch(target: &mut TcpStream, pane: &str) {
    target.set_read_timeout(Some(WAIT)).unwrap();
    let message: ClientMsg = codec::decode(target).unwrap();
    assert!(matches!(message, ClientMsg::Watch { pane: shown, .. } if shown == pane));
}

fn client_barrier(client: &mut TcpStream) {
    send(
        client,
        &ClientMsg::DetachClient {
            client_id: String::new(),
        },
    );
    loop {
        match decode(client) {
            ServerMsg::Clients { .. } => return,
            ServerMsg::Refused { .. } => panic!("watch restore must succeed when the target reads"),
            _ => {}
        }
    }
}

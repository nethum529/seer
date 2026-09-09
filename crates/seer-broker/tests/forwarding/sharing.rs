use super::*;

#[test]
fn watch_receives_another_persons_cells_without_a_grant() {
    let files = TestFiles::new();
    let address = unused_address();
    write_config(&files, address);
    files.write_runtime_wrapper();
    let _broker = ProcessGuard::new(files.start_broker());
    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    let tree = wait_for_tree_with_tab(&mut alice);
    let pane = tree.workspaces[0].tabs[0].panes[0].id.clone();
    let mut bob = connect_when_ready(address);
    send_hello(&mut bob, "bob", "bob-secret");
    wait_for_tree_with_tab(&mut bob);
    send(
        &mut bob,
        &ClientMsg::Watch {
            cols: 80,
            rows: 24,
            user: "alice".into(),
            pane: pane.clone(),
        },
    );
    let cells = wait_for_broker_message(
        &mut bob,
        |message| matches!(message, ServerMsg::Cells { user, pane: received, .. } if user == "alice" && received == &pane),
    );
    assert!(matches!(cells, ServerMsg::Cells { .. }));
}

#[test]
fn type_into_requires_a_grant_and_names_the_sender_without_changing_bytes() {
    let files = TestFiles::new();
    let address = unused_address();
    write_config(&files, address);
    files.write_runtime_wrapper();
    let _broker = ProcessGuard::new(files.start_broker());
    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    let tree = wait_for_tree_with_tab(&mut alice);
    let pane = tree.workspaces[0].tabs[0].panes[0].id.clone();
    let mut bob = connect_when_ready(address);
    send_hello(&mut bob, "bob", "bob-secret");
    wait_for_tree_with_tab(&mut bob);
    let input = ClientMsg::TypeInto {
        user: "alice".into(),
        pane,
        bytes: b"printf '%s%s\\n' marker free\n".to_vec(),
    };
    send(&mut bob, &input);
    assert!(matches!(
        wait_for_refused(&mut bob),
        ServerMsg::Refused { .. }
    ));
    send(
        &mut alice,
        &ClientMsg::SetGrant {
            user: "bob".into(),
            can_type: true,
        },
    );
    wait_for_broker_message(
        &mut bob,
        |message| matches!(message, ServerMsg::Grants { you_may_type_into, .. } if you_may_type_into.contains(&"alice".to_owned())),
    );
    send(
        &mut bob,
        &ClientMsg::Terminals {
            user: "alice".into(),
        },
    );
    wait_for_broker_message(
        &mut bob,
        |message| matches!(message, ServerMsg::Terminals { user, terminals } if user == "alice" && !terminals.is_empty()),
    );
    send(&mut bob, &input);
    let state = wait_for_broker_message(&mut bob, |message| {
        let value = serde_json::to_value(message).unwrap();
        value["Terminals"]["terminals"]
            .as_array()
            .is_some_and(|terminals| {
                terminals
                    .iter()
                    .any(|terminal| terminal["last_typist"] == "bob")
            })
    });
    assert!(matches!(state, ServerMsg::Terminals { .. }));
    let text = wait_for_cells_containing(&mut alice, "markerfree");
    assert!(!text.contains("[seer:"));
}

#[test]
fn grant_survives_a_broker_restart() {
    let files = TestFiles::new();
    let address = unused_address();
    write_config(&files, address);
    files.write_runtime_wrapper();
    let broker = ProcessGuard::new(files.start_broker());
    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    wait_for_tree_with_tab(&mut alice);
    send(
        &mut alice,
        &ClientMsg::SetGrant {
            user: "bob".into(),
            can_type: true,
        },
    );
    wait_for_broker_message(
        &mut alice,
        |message| matches!(message, ServerMsg::Grants { can_type_here, .. } if can_type_here.contains(&"bob".to_owned())),
    );
    drop(alice);
    drop(broker);
    let _broker = ProcessGuard::new(files.start_broker());
    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    welcome_client_id(read_message(&mut alice), "alice");
    assert!(
        matches!(read_message(&mut alice), ServerMsg::Grants { can_type_here, .. } if can_type_here == vec!["bob"])
    );
}

#[test]
fn the_active_own_client_holds_the_pane_size_against_remote_watchers() {
    let files = TestFiles::new();
    let address = unused_address();
    write_config(&files, address);
    files.write_runtime_wrapper();
    let _broker = ProcessGuard::new(files.start_broker());
    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    let tree = wait_for_tree_with_tab(&mut alice);
    let pane = tree.workspaces[0].tabs[0].panes[0].id.clone();
    let mut bob = connect_when_ready(address);
    send_hello(&mut bob, "bob", "bob-secret");
    wait_for_tree_with_tab(&mut bob);
    let mut bob_reader = bob.try_clone().unwrap();
    let reader = thread::spawn(move || {
        while seer_core::proto::codec::decode::<_, ServerMsg>(&mut bob_reader).is_ok() {}
    });
    for (stream, cols, rows) in [(&mut alice, 120, 40), (&mut bob, 90, 50)] {
        let watch = serde_json::json!({"Watch": {"user": "alice", "pane": pane, "cols": cols, "rows": rows}});
        seer_core::proto::codec::encode(stream, &watch).unwrap();
    }
    // Bob is a remote watcher, so alice keeps control. The old smallest rule would give 90 columns here.
    let watch =
        serde_json::json!({"Watch": {"user": "alice", "pane": pane, "cols": 100, "rows": 30}});
    seer_core::proto::codec::encode(&mut alice, &watch).unwrap();
    wait_for_size(&mut alice, &pane, 100, 30);
    send(
        &mut alice,
        &ClientMsg::Unwatch {
            user: "alice".into(),
            pane: pane.clone(),
        },
    );
    wait_for_size(&mut alice, &pane, 90, 50);
    let watch =
        serde_json::json!({"Watch": {"user": "alice", "pane": pane, "cols": 60, "rows": 20}});
    seer_core::proto::codec::encode(&mut bob, &watch).unwrap();
    wait_for_size(&mut alice, &pane, 60, 20);
    bob.shutdown(std::net::Shutdown::Both).unwrap();
    reader.join().unwrap();
    drop(bob);
    wait_for_size(&mut alice, &pane, 100, 30);
}

fn wait_for_size(stream: &mut TcpStream, pane: &str, cols: u16, rows: u16) {
    wait_for_broker_message(stream, |message| {
        matches!(message,
        ServerMsg::Cells { user, pane: id, frame } if user == "alice" && id == pane
        && frame.rows.len() == usize::from(rows)
        && frame.rows.first().is_some_and(|row| row.len() == usize::from(cols)))
    });
}

#[test]
fn remote_directory_stays_complete_while_watching_a_subset() {
    let files = TestFiles::new();
    let address = unused_address();
    write_config(&files, address);
    files.write_runtime_wrapper();
    let _broker = ProcessGuard::new(files.start_broker());
    let mut alice = connect_when_ready(address);
    send_hello(&mut alice, "alice", "alice-secret");
    wait_for_tree_with_tab(&mut alice);
    for _ in 0..2 {
        send(
            &mut alice,
            &ClientMsg::CreateTab {
                workspace: "w1".into(),
            },
        );
    }
    let tabs = wait_for_tab_count(&mut alice, 3);
    let mut bob = connect_when_ready(address);
    send_hello(&mut bob, "bob", "bob-secret");
    let own = wait_for_tree_with_tab(&mut bob).workspaces[0].tabs[0].panes[0]
        .id
        .clone();
    send(
        &mut bob,
        &ClientMsg::Terminals {
            user: "alice".into(),
        },
    );
    let mut catalog = Catalog {
        watched: Vec::new(),
        size: 0,
        own,
    };
    catalog.wait_for_directory(&mut bob, 3);
    for ((_, pane), cols) in tabs[..2].iter().zip([100, 90]) {
        send(
            &mut bob,
            &ClientMsg::Watch {
                user: "alice".into(),
                pane: pane.clone(),
                cols,
                rows: 30,
            },
        );
        catalog.watched.push(pane.clone());
        catalog.wait_for(&mut bob, |message| {
            matches!(message, ServerMsg::Cells { user, pane: id, .. } if user == "alice" && id == pane)
        });
        catalog.wait_for_directory(&mut bob, 3);
    }
    add_and_close_unwatched(&mut alice, &mut bob, &mut catalog, 3);
    let (tab, pane) = tabs[1].clone();
    close_tab(&mut alice, &tab, &pane);
    wait_for_tab_count(&mut alice, 2);
    catalog.wait_for_directory(&mut bob, 2);
    catalog.watched.retain(|id| id != &pane);
    for pane in catalog.watched.clone() {
        send(
            &mut bob,
            &ClientMsg::Unwatch {
                user: "alice".into(),
                pane,
            },
        );
    }
    send(
        &mut bob,
        &ClientMsg::QueryTargets {
            user: "alice".into(),
        },
    );
    catalog.wait_for(&mut bob, |message| {
        matches!(message, ServerMsg::Targets { .. })
    });
    catalog.watched.clear();
    add_and_close_unwatched(&mut alice, &mut bob, &mut catalog, 2);
}

fn add_and_close_unwatched(
    alice: &mut TcpStream,
    bob: &mut TcpStream,
    catalog: &mut Catalog,
    count: usize,
) {
    send(
        alice,
        &ClientMsg::CreateTab {
            workspace: "w1".into(),
        },
    );
    let tabs = wait_for_tab_count(alice, count + 1);
    catalog.wait_for_directory(bob, count + 1);
    let (tab, pane) = tabs[count].clone();
    close_tab(alice, &tab, &pane);
    wait_for_tab_count(alice, count);
    catalog.wait_for_directory(bob, count);
}

fn close_tab(alice: &mut TcpStream, tab: &str, pane: &str) {
    send(
        alice,
        &ClientMsg::ClosePane {
            workspace: "w1".into(),
            tab: tab.into(),
            pane: pane.into(),
        },
    );
}

fn wait_for_tab_count(stream: &mut TcpStream, count: usize) -> Vec<(String, String)> {
    let tree = wait_for_broker_message(
        stream,
        |message| matches!(message, ServerMsg::Tree { tree } if tree.workspaces[0].tabs.len() == count),
    );
    let ServerMsg::Tree { tree } = tree else {
        unreachable!()
    };
    tree.workspaces[0]
        .tabs
        .iter()
        .map(|tab| (tab.id.clone(), tab.panes[0].id.clone()))
        .collect()
}

struct Catalog {
    watched: Vec<String>,
    size: usize,
    own: String,
}

impl Catalog {
    fn wait_for(&self, stream: &mut TcpStream, expected: impl Fn(&ServerMsg) -> bool) {
        wait_for_broker_message(stream, |message| {
            match message {
                ServerMsg::Cells { user, pane, .. } if user == "alice" => {
                    assert!(
                        self.watched.contains(pane),
                        "cells for unwatched pane {pane}"
                    );
                }
                ServerMsg::Frame { pane, .. } => {
                    assert!(
                        pane == &self.own || self.watched.contains(pane),
                        "frame for unwatched pane {pane}"
                    );
                }
                _ => {}
            }
            expected(message)
        });
    }

    fn wait_for_directory(&mut self, stream: &mut TcpStream, count: usize) {
        let last = self.size;
        self.wait_for(stream, |message| match message {
            ServerMsg::Terminals { user, terminals } if user == "alice" => {
                assert!(
                    terminals.len() == count || terminals.len() == last,
                    "directory listed {} terminals",
                    terminals.len()
                );
                terminals.len() == count
            }
            _ => false,
        });
        self.size = count;
    }
}

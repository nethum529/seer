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
fn pane_uses_smallest_visible_size_and_recovers_after_unwatch_or_disconnect() {
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
    wait_for_size(&mut alice, &pane, 90, 40);
    send(
        &mut bob,
        &ClientMsg::Unwatch {
            user: "alice".into(),
            pane: pane.clone(),
        },
    );
    wait_for_size(&mut alice, &pane, 120, 40);
    let watch =
        serde_json::json!({"Watch": {"user": "alice", "pane": pane, "cols": 60, "rows": 20}});
    seer_core::proto::codec::encode(&mut bob, &watch).unwrap();
    wait_for_size(&mut alice, &pane, 60, 20);
    bob.shutdown(std::net::Shutdown::Both).unwrap();
    reader.join().unwrap();
    drop(bob);
    wait_for_size(&mut alice, &pane, 120, 40);
    send(
        &mut alice,
        &ClientMsg::Unwatch {
            user: "alice".into(),
            pane: pane.clone(),
        },
    );
    wait_for_size(&mut alice, &pane, 80, 24);
}

fn wait_for_size(stream: &mut TcpStream, pane: &str, cols: u16, rows: u16) {
    wait_for_broker_message(stream, |message| {
        matches!(message,
        ServerMsg::Cells { user, pane: id, frame } if user == "alice" && id == pane
        && frame.rows.len() == usize::from(rows)
        && frame.rows.first().is_some_and(|row| row.len() == usize::from(cols)))
    });
}

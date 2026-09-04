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
fn type_into_requires_a_grant_and_marks_each_line() {
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
        bytes: b"first-line\nsecond-line\n".to_vec(),
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
    let text = wait_for_cells_containing(&mut alice, "[seer: bob] second-line");
    assert!(text.contains("[seer: bob] first-line"));
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

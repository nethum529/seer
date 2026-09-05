use super::*;

#[test]
#[ignore = "prints latency samples; run with --ignored --nocapture"]
fn input_to_both_screens_latency() {
    let files = TestFiles::new();
    let address = unused_address();
    write_config(&files, address);
    files.write_runtime_wrapper();
    let _broker = ProcessGuard::new(files.start_broker());
    let mut owner = connect_when_ready(address);
    send_hello(&mut owner, "alice", "alice-secret");
    let tree = wait_for_tree_with_tab(&mut owner);
    let pane = tree.workspaces[0].tabs[0].panes[0].id.clone();
    send_input(
        &mut owner,
        "w1",
        "w1:t1",
        &pane,
        "stty -echo -icanon; printf '\\033c%s%s\\n' SEER_ LATENCY_READY; exec cat\n",
    );
    wait_for_cells_containing(&mut owner, "SEER_LATENCY_READY");
    let mut watcher = connect_when_ready(address);
    send_hello(&mut watcher, "bob", "bob-secret");
    wait_for_tree_with_tab(&mut watcher);
    send(
        &mut owner,
        &ClientMsg::SetGrant {
            user: "bob".into(),
            can_type: true,
        },
    );
    wait_for_broker_message(&mut watcher, |message| {
        matches!(message,
        ServerMsg::Grants { you_may_type_into, .. } if you_may_type_into.contains(&"alice".into()))
    });
    send(
        &mut watcher,
        &ClientMsg::Watch {
            cols: 80,
            rows: 24,
            user: "alice".into(),
            pane: pane.clone(),
        },
    );
    let initial = wait_for_broker_message(&mut watcher, |message| {
        matches!(message,
        ServerMsg::Cells { user, .. } if user == "alice")
    });
    if let ServerMsg::Cells { frame, .. } = initial {
        println!(
            "full_frame_bytes={} full_rows_bytes={} dirty_row_bytes={}",
            serde_json::to_vec(&frame).unwrap().len(),
            serde_json::to_vec(&frame.rows).unwrap().len(),
            serde_json::to_vec(&frame.rows[0]).unwrap().len()
        );
    }
    let mut expected = String::new();
    for key in b"abcdefghijklmnopqrst" {
        expected.push(char::from(*key));
        let start = Instant::now();
        println!(
            "client_type {}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        send(
            &mut watcher,
            &ClientMsg::TypeInto {
                user: "alice".into(),
                pane: pane.clone(),
                bytes: vec![*key],
            },
        );
        thread::scope(|scope| {
            let owner_expected = &expected;
            let owner_stream = &mut owner;
            let owner_read = scope.spawn(move || {
                wait_for_cells_containing(owner_stream, owner_expected);
                start.elapsed()
            });
            wait_for_cells_containing(&mut watcher, &expected);
            let watcher_elapsed = start.elapsed();
            println!(
                "latency owner_us={} watcher_us={}",
                owner_read.join().unwrap().as_micros(),
                watcher_elapsed.as_micros()
            );
        });
    }
    let log = std::fs::read_to_string(&files.broker_log).unwrap();
    for line in log.lines().filter(|line| line.contains("SEER_LATENCY")) {
        println!("{line}");
    }
}

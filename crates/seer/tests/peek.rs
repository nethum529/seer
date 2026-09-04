use std::thread;

use seer_core::proto::{ClientMsg, PeekTarget, ServerMsg};

use super::{TestConfig, accept, listener, person, run, saved, send, send_welcome};

#[test]
fn peek_queries_an_exact_person_and_sends_the_selected_target() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    super::write_store(&config, &[saved(address.port(), "team.example.com", true)]);
    let server = thread::spawn(move || {
        for exact in [false, true] {
            let mut stream = accept(&listener);
            super::assert_hello(&mut stream);
            send_welcome(&mut stream, "user-bob", "bob");
            assert_eq!(super::receive(&mut stream), ClientMsg::ListPeople);
            send(
                &mut stream,
                &ServerMsg::People {
                    people: vec![person("user-alice", "alice", 1)],
                },
            );
            if exact {
                assert_eq!(
                    super::receive(&mut stream),
                    ClientMsg::QueryTargets {
                        user: "user-alice".into(),
                    }
                );
                send(
                    &mut stream,
                    &ServerMsg::Targets {
                        targets: vec![PeekTarget {
                            workspace: "w9".into(),
                            workspace_name: "work".into(),
                            tab: "w9:t7".into(),
                            tab_title: "shell".into(),
                        }],
                    },
                );
                assert_eq!(
                    super::receive(&mut stream),
                    ClientMsg::Peek {
                        user: "user-alice".into(),
                        workspace: "w9".into(),
                        tab: "w9:t7".into(),
                    }
                );
            }
        }
    });

    let close = run(&config, &["peek", "alic"], "");
    assert_eq!(close.status.code(), Some(1));
    assert!(close.stdout.is_empty());
    assert_eq!(close.stderr, b"Close names: alice\nno person named alic\n");

    let exact = run(&config, &["peek", "alice"], "");
    assert_eq!(exact.status.code(), Some(0));
    assert_eq!(
        exact.stdout,
        b"PEEK: alice - READ ONLY\nWorkspace: alice/w9\n"
    );
    assert!(exact.stderr.is_empty());
    server.join().expect("server must finish");
}

#[test]
fn peek_presents_a_picker_and_sends_opaque_target_ids() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    super::write_store(&config, &[saved(address.port(), "team.example.com", true)]);
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        super::assert_hello(&mut stream);
        send_welcome(&mut stream, "user-bob", "bob");
        assert_eq!(super::receive(&mut stream), ClientMsg::ListPeople);
        send(
            &mut stream,
            &ServerMsg::People {
                people: vec![person("user-alice", "alice", 1)],
            },
        );
        assert_eq!(
            super::receive(&mut stream),
            ClientMsg::QueryTargets {
                user: "user-alice".into(),
            }
        );
        send(
            &mut stream,
            &ServerMsg::Targets {
                targets: vec![
                    PeekTarget {
                        workspace: "w1".into(),
                        workspace_name: "main".into(),
                        tab: "w1:t8".into(),
                        tab_title: "shell".into(),
                    },
                    PeekTarget {
                        workspace: "w4".into(),
                        workspace_name: "work".into(),
                        tab: "w4:t9".into(),
                        tab_title: "tests".into(),
                    },
                ],
            },
        );
        assert_eq!(
            super::receive(&mut stream),
            ClientMsg::Peek {
                user: "user-alice".into(),
                workspace: "w4".into(),
                tab: "w4:t9".into(),
            }
        );
    });

    let output = run(&config, &["peek", "alice"], "2\n");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        super::text(&output.stdout),
        "Select a target:\n  1. main/shell\n  2. work/tests\nTarget: PEEK: alice - READ ONLY\nWorkspace: alice/w4\n"
    );
    assert!(output.stderr.is_empty());
    server.join().expect("server must finish");
}

#[test]
fn peek_rejects_a_person_without_targets() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    super::write_store(&config, &[saved(address.port(), "team.example.com", true)]);
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        super::assert_hello(&mut stream);
        send_welcome(&mut stream, "user-bob", "bob");
        assert_eq!(super::receive(&mut stream), ClientMsg::ListPeople);
        send(
            &mut stream,
            &ServerMsg::People {
                people: vec![person("user-alice", "alice", 0)],
            },
        );
        assert_eq!(
            super::receive(&mut stream),
            ClientMsg::QueryTargets {
                user: "user-alice".into(),
            }
        );
        send(
            &mut stream,
            &ServerMsg::Targets {
                targets: Vec::new(),
            },
        );
    });

    let output = run(&config, &["peek", "alice"], "");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"no active target\n");
    server.join().expect("server must finish");
}

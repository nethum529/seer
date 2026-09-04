use super::*;
use seer_core::{InputEvent, SplitDirection, TerminalInput};
use std::thread;
use std::time::{Duration, Instant};

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[test]
fn splits_and_resizes_both_panes() {
    let mut session = session_with_tab();
    session
        .apply(ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 81,
            rows: 25,
        })
        .expect("resize must succeed");

    let messages = session
        .apply(ClientMsg::SplitPane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            direction: SplitDirection::Right,
        })
        .expect("split must succeed");

    let panes = &message_tree(&messages).workspaces[0].tabs[0].panes;
    assert_eq!(panes.len(), 2);
    assert_eq!(panes[0].size, PaneSize { cols: 41, rows: 25 });
    assert_eq!(panes[1].size, PaneSize { cols: 40, rows: 25 });
    close_all_panes(&mut session);
}

#[test]
fn poll_omits_a_quiet_pane() {
    let mut session = session_with_tab();
    assert!(wait_for_cells(&mut session, |_| true).is_some());

    assert!(session.poll().is_empty());
    close_all_panes(&mut session);
}

#[test]
fn closes_a_pane_and_kills_its_process() {
    let mut session = session_with_tab();
    session
        .apply(ClientMsg::SplitPane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            direction: SplitDirection::Down,
        })
        .expect("split must succeed");
    assert!(session.pane_hosts.contains_key("w1:p2"));
    let pid_file = std::env::temp_dir().join(format!("seer-runtime-close-{}", std::process::id()));
    session
        .apply(ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p2".into(),
            input: TerminalInput::new(InputEvent::Text(format!(
                "echo $$ > {}\n",
                pid_file.display()
            ))),
        })
        .expect("PID command must succeed");
    let shell_pid = wait_for_pid(&pid_file).expect("shell PID must be valid");

    let messages = session
        .apply(ClientMsg::ClosePane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p2".into(),
        })
        .expect("close must succeed");

    assert!(!session.pane_hosts.contains_key("w1:p2"));
    assert!(wait_for_process_stop(shell_pid));
    std::fs::remove_file(pid_file).expect("PID file must be removed");
    assert_eq!(message_tree(&messages).workspaces[0].tabs[0].panes.len(), 1);
    assert_eq!(
        session.tree.workspaces[0].tabs[0].panes[0].size,
        PaneSize { cols: 80, rows: 24 }
    );
    close_all_panes(&mut session);
}

#[test]
fn focuses_a_pane_and_ignores_deferred_messages() {
    let mut session = session_with_tab();
    session
        .apply(ClientMsg::SplitPane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            direction: SplitDirection::Right,
        })
        .expect("split must succeed");

    let messages = session
        .apply(ClientMsg::FocusPane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p1".into(),
        })
        .expect("focus must succeed");
    assert_eq!(
        message_tree(&messages).workspaces[0].tabs[0]
            .layout
            .focused
            .as_deref(),
        Some("w1:p1")
    );

    let deferred = [
        ClientMsg::Hello {
            user_id: "alice".into(),
            credential: "secret".into(),
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
        ClientMsg::QueryTargets { user: "bob".into() },
        ClientMsg::Peek {
            user: "bob".into(),
            workspace: "w1".into(),
            tab: "w1:t1".into(),
        },
        ClientMsg::StopPeek,
        ClientMsg::Detach,
    ];
    for message in deferred {
        assert!(
            session
                .apply(message)
                .expect("message must succeed")
                .is_empty()
        );
    }
    close_all_panes(&mut session);
}

#[test]
fn peek_selects_one_workspace_and_tab_and_rejects_invalid_ids() {
    let mut session = UserSession::new("alice", "sh");
    for name in ["first", "second"] {
        let workspace = session
            .tree
            .create_workspace(name)
            .expect("workspace must be created");
        session
            .tree
            .create_tab(&workspace.id, "first", session.viewport)
            .expect("first tab must be created");
        session
            .tree
            .create_tab(&workspace.id, "second", session.viewport)
            .expect("second tab must be created");
    }

    let selected = session
        .selected_tree("w2", "w2:t2")
        .expect("selected tree must exist");

    assert_eq!(selected.workspaces.len(), 1);
    assert_eq!(selected.workspaces[0].id, "w2");
    assert_eq!(selected.workspaces[0].tabs.len(), 1);
    assert_eq!(selected.workspaces[0].tabs[0].id, "w2:t2");
    assert_eq!(
        session
            .selected_tree("w9", "w9:t1")
            .expect_err("workspace must be rejected")
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        session
            .selected_tree("w1", "w2:t1")
            .expect_err("tab must be rejected")
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

fn session_with_tab() -> UserSession {
    let mut session = UserSession::new("alice", "sh");
    session
        .ensure_first_shell()
        .expect("first shell must be created");
    session
}

fn message_tree(messages: &[ServerMsg]) -> &Tree {
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        ServerMsg::Tree { tree } => tree,
        message => panic!("expected tree message, got {message:?}"),
    }
}

fn wait_for_cells(
    session: &mut UserSession,
    matches: impl Fn(&str) -> bool,
) -> Option<Vec<ServerMsg>> {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    let mut matched_messages = None;
    while Instant::now() < deadline {
        let messages = session.poll();
        if messages.is_empty() && matched_messages.is_some() {
            return matched_messages;
        }
        let text = cells_text(&messages);
        if !messages.is_empty() && matches(&text) {
            matched_messages = Some(messages);
        }
        thread::sleep(POLL_INTERVAL);
    }
    None
}

fn wait_for_pid(path: &std::path::Path) -> Option<u32> {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if path.is_file()
            && let Ok(contents) = std::fs::read_to_string(path)
            && !contents.trim().is_empty()
            && let Ok(pid) = contents.trim().parse::<u32>()
        {
            return Some(pid);
        }
        thread::sleep(POLL_INTERVAL);
    }
    None
}

fn wait_for_process_stop(pid: u32) -> bool {
    let status = format!("/proc/{pid}/status");
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline {
        match std::fs::read_to_string(&status) {
            Ok(contents) if contents.lines().any(|line| line.starts_with("State:\tZ")) => {
                return true;
            }
            Ok(_) => thread::sleep(POLL_INTERVAL),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return true,
            Err(_) => return false,
        }
    }
    false
}

fn cells_text(messages: &[ServerMsg]) -> String {
    messages
        .iter()
        .filter_map(|message| match message {
            ServerMsg::Cells { frame, .. } => Some(&frame.rows),
            _ => None,
        })
        .flatten()
        .flatten()
        .map(|cell| cell.character)
        .collect()
}

fn close_all_panes(session: &mut UserSession) {
    let pane_ids: Vec<String> = session.pane_hosts.keys().cloned().collect();
    for pane in pane_ids {
        session
            .apply(ClientMsg::ClosePane {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                pane,
            })
            .expect("pane close must succeed");
    }
}

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Rect, Size};
use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{
    Cursor, InputEvent, KeyCode as CoreKeyCode, KeyInput, Modifiers, MouseTracking, PaneSize,
    TerminalFrame, TerminalInput, TerminalModes, Tree,
};

use super::test_support::*;
use super::{
    LoopControl, apply_server_message, draw, handle_event, set_peek_person, set_view_only,
};
use crate::drawer::Drawer;
use crate::state::ClientState;

#[test]
fn view_only_events_send_no_session_changes() {
    let (mut client, mut server) = socket_pair();
    let mut state = state_with_pane();
    let mut command_pending = false;
    let mut drawer = Drawer::default();
    set_view_only(true);
    let events = [
        Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
        Event::Key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)),
        Event::Resize(120, 40),
    ];

    for event in events {
        assert_eq!(
            handle_event(
                event,
                &mut client,
                &mut state,
                &mut command_pending,
                Size::new(80, 24),
                &mut drawer,
            )
            .expect("event handling must succeed"),
            LoopControl::Continue
        );
    }

    assert_no_message(&mut server);

    assert_eq!(
        handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            &mut client,
            &mut state,
            &mut command_pending,
            Size::new(80, 24),
            &mut drawer,
        )
        .expect("detach key must be handled"),
        LoopControl::Exit
    );
    assert_eq!(decode(&mut server), ClientMsg::Detach);
}

#[test]
fn active_events_send_input_focus_and_resize() {
    let (mut client, mut server) = socket_pair();
    let mut tree = tree_with_two_tabs();
    let mut state = ClientState::new(tree.clone());
    let mut command_pending = false;
    let mut drawer = Drawer::default();
    set_view_only(false);

    handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(80, 24),
        &mut drawer,
    )
    .expect("input key must be handled");
    handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(80, 24),
        &mut drawer,
    )
    .expect("command prefix must be handled");
    handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(80, 24),
        &mut drawer,
    )
    .expect("focus key must be handled");
    handle_event(
        Event::Resize(120, 40),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(120, 40),
        &mut drawer,
    )
    .expect("resize must be handled");
    let release = KeyEvent::new_with_kind(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );
    assert_eq!(
        handle_event(
            Event::Key(release),
            &mut client,
            &mut state,
            &mut command_pending,
            Size::new(80, 24),
            &mut drawer,
        )
        .expect("release key must be handled"),
        LoopControl::Continue
    );
    for key in [
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
    ] {
        handle_event(
            Event::Key(key),
            &mut client,
            &mut state,
            &mut command_pending,
            Size::new(80, 24),
            &mut drawer,
        )
        .expect("drawer key must be handled");
    }
    assert!(drawer.is_open());
    apply_two_people(&mut client, &mut state, &mut drawer);
    assert_eq!(highlighted_person(&drawer, &mut state), Some(0));
    send_test_key(
        &mut client,
        &mut state,
        &mut command_pending,
        KeyCode::Down,
        &mut drawer,
    );
    assert_eq!(highlighted_person(&drawer, &mut state), Some(1));
    send_test_key(
        &mut client,
        &mut state,
        &mut command_pending,
        KeyCode::Char('z'),
        &mut drawer,
    );
    handle_event(
        Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(80, 24),
        &mut drawer,
    )
    .expect("drawer escape must be handled");
    assert!(!drawer.is_open());

    assert_eq!(
        decode(&mut server),
        ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p1".into(),
            input: TerminalInput::new(InputEvent::Key(KeyInput {
                code: CoreKeyCode::Char('a'),
                modifiers: Modifiers::default(),
            })),
        }
    );
    assert_eq!(
        decode(&mut server),
        ClientMsg::FocusPane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p1".into(),
        }
    );
    assert_eq!(
        decode(&mut server),
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 119,
            rows: 40,
        }
    );
    assert_eq!(decode(&mut server), ClientMsg::ListPeople);
    assert_key_input(&mut server, "w1:p1", 'z');

    send_prefixed_key(
        &mut client,
        &mut state,
        &mut command_pending,
        KeyCode::Char('%'),
        &mut drawer,
    );
    assert_eq!(
        decode(&mut server),
        ClientMsg::SplitPane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            direction: seer_core::SplitDirection::Right,
        }
    );
    tree.split_pane("w1:p1", seer_core::SplitDirection::Right)
        .expect("right split must be created");
    apply_server_message(
        ServerMsg::Tree { tree: tree.clone() },
        &mut state,
        &mut client,
        Size::new(120, 40),
        &mut drawer,
    )
    .expect("split tree must apply");
    send_test_key(
        &mut client,
        &mut state,
        &mut command_pending,
        KeyCode::Char('h'),
        &mut drawer,
    );
    assert_key_input(&mut server, "w1:p3", 'h');

    send_prefixed_key(
        &mut client,
        &mut state,
        &mut command_pending,
        KeyCode::Char('"'),
        &mut drawer,
    );
    assert_eq!(
        decode(&mut server),
        ClientMsg::SplitPane {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            direction: seer_core::SplitDirection::Down,
        }
    );
    tree.split_pane("w1:p3", seer_core::SplitDirection::Down)
        .expect("down split must be created");
    apply_server_message(
        ServerMsg::Tree { tree: tree.clone() },
        &mut state,
        &mut client,
        Size::new(120, 40),
        &mut drawer,
    )
    .expect("split tree must apply");
    send_test_key(
        &mut client,
        &mut state,
        &mut command_pending,
        KeyCode::Char('v'),
        &mut drawer,
    );
    assert_key_input(&mut server, "w1:p4", 'v');

    for (key, pane, marker) in [
        (KeyCode::Up, "w1:p3", 'u'),
        (KeyCode::Down, "w1:p4", 'd'),
        (KeyCode::Left, "w1:p1", 'l'),
        (KeyCode::Right, "w1:p3", 'r'),
    ] {
        send_prefixed_key(
            &mut client,
            &mut state,
            &mut command_pending,
            key,
            &mut drawer,
        );
        assert_eq!(
            decode(&mut server),
            ClientMsg::FocusPane {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                pane: pane.into(),
            }
        );
        tree.focus_pane(pane).expect("pane focus must change");
        apply_server_message(
            ServerMsg::Tree { tree: tree.clone() },
            &mut state,
            &mut client,
            Size::new(120, 40),
            &mut drawer,
        )
        .expect("focus tree must apply");
        send_test_key(
            &mut client,
            &mut state,
            &mut command_pending,
            KeyCode::Char(marker),
            &mut drawer,
        );
        assert_key_input(&mut server, pane, marker);
    }

    send_prefixed_key(
        &mut client,
        &mut state,
        &mut command_pending,
        KeyCode::Char('n'),
        &mut drawer,
    );
    assert_eq!(
        decode(&mut server),
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t2".into(),
            cols: 119,
            rows: 40,
        }
    );
    let mut tree = Tree::new();
    tree.create_workspace("main")
        .expect("workspace must be created");
    for title in ["first", "removed", "third"] {
        tree.create_tab("w1", title, PaneSize { cols: 80, rows: 24 })
            .expect("tab must be created");
    }
    tree.close_tab("w1", "w1:t2")
        .expect("selected tab must close");
    apply_server_message(
        ServerMsg::Tree { tree },
        &mut state,
        &mut client,
        Size::new(120, 40),
        &mut drawer,
    )
    .expect("tree must apply");
    assert_eq!(
        decode(&mut server),
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 119,
            rows: 40,
        }
    );
    assert_no_message(&mut server);
}

#[test]
fn mouse_move_without_tracking_sends_no_message() {
    let (mut client, mut server) = socket_pair();
    let mut state = state_with_pane();
    let mut command_pending = false;
    let mut drawer = Drawer::default();
    set_view_only(false);
    state.set_pane_areas(vec![("w1:p1".into(), Rect::new(0, 0, 80, 24))]);
    state.apply_frame(
        "w1:p1".into(),
        TerminalFrame {
            rows: Vec::new(),
            cursor: Cursor::default(),
            modes: TerminalModes {
                mouse_tracking: MouseTracking::Click,
            },
        },
    );

    handle_event(
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 10,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(80, 24),
        &mut drawer,
    )
    .expect("mouse move must be handled");

    handle_event(
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 79,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(80, 24),
        &mut drawer,
    )
    .expect("drawer handle click must be handled");

    assert!(drawer.is_open());
    assert_eq!(decode(&mut server), ClientMsg::ListPeople);
}

#[test]
fn detached_bye_has_a_distinct_exit() {
    let (mut client, _) = socket_pair();
    let mut state = state_with_pane();
    let mut drawer = Drawer::default();

    assert_eq!(
        apply_server_message(
            ServerMsg::Bye {
                reason: "detached".into(),
            },
            &mut state,
            &mut client,
            Size::new(80, 24),
            &mut drawer,
        )
        .expect("Bye must apply"),
        LoopControl::Detached
    );
    assert_eq!(
        apply_server_message(
            ServerMsg::Bye {
                reason: "server stopped".into(),
            },
            &mut state,
            &mut client,
            Size::new(80, 24),
            &mut drawer,
        )
        .expect("Bye must apply"),
        LoopControl::Exit
    );
}

#[test]
fn peek_banner_and_drawer_are_fixed_over_the_tree() {
    let mut terminal = Terminal::new(TestBackend::new(80, 5)).expect("terminal must start");
    let mut state = state_with_pane();
    let mut drawer = Drawer::default();
    set_peek_person(Some("alice"));

    terminal
        .draw(|frame| draw(frame, &mut state, &drawer))
        .expect("frame must draw");

    let buffer = terminal.backend().buffer();
    let first_line: String = (0..79).map(|x| buffer[(x, 0)].symbol()).collect();
    let second_line: String = (0..79).map(|x| buffer[(x, 1)].symbol()).collect();
    assert_eq!(first_line.trim_end(), "PEEK: alice - READ ONLY");
    assert_eq!(second_line.trim_end(), "Workspace: alice/w1");
    assert_eq!(buffer[(78, 2)].symbol(), "\u{2510}");
    assert_eq!(buffer[(79, 2)].symbol(), "\u{2502}");

    drawer.toggle();
    terminal
        .draw(|frame| draw(frame, &mut state, &drawer))
        .expect("open drawer must draw");
    let title: String = (39..79)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
        .collect();
    assert!(title.starts_with("\u{250c}People"));
    set_peek_person(None);
}

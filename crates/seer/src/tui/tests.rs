use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Rect, Size};
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{
    Cursor, InputEvent, KeyCode as CoreKeyCode, KeyInput, Modifiers, MouseTracking, PaneSize,
    TerminalFrame, TerminalInput, TerminalModes, Tree,
};

use super::{
    LoopControl, apply_server_message, draw, handle_event, set_peek_person, set_view_only,
};
use crate::state::ClientState;

#[test]
fn view_only_events_send_no_session_changes() {
    let (mut client, mut server) = socket_pair();
    let mut state = state_with_pane();
    let mut command_pending = false;
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
        )
        .expect("detach key must be handled"),
        LoopControl::Exit
    );
    assert_eq!(decode(&mut server), ClientMsg::Detach);
}

#[test]
fn active_events_send_input_focus_and_resize() {
    let (mut client, mut server) = socket_pair();
    let mut state = state_with_two_tabs();
    let mut command_pending = false;
    set_view_only(false);

    handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(80, 24),
    )
    .expect("input key must be handled");
    handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(80, 24),
    )
    .expect("command prefix must be handled");
    handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(80, 24),
    )
    .expect("focus key must be handled");
    handle_event(
        Event::Resize(120, 40),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(120, 40),
    )
    .expect("resize must be handled");
    handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(120, 40),
    )
    .expect("command prefix must be handled");
    handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
        &mut client,
        &mut state,
        &mut command_pending,
        Size::new(120, 40),
    )
    .expect("tab change must be handled");
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
        )
        .expect("release key must be handled"),
        LoopControl::Continue
    );

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
            cols: 120,
            rows: 40,
        }
    );
    assert_eq!(
        decode(&mut server),
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t2".into(),
            cols: 120,
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
    )
    .expect("tree must apply");
    assert_eq!(
        decode(&mut server),
        ClientMsg::Resize {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            cols: 120,
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
    )
    .expect("mouse move must be handled");

    assert_no_message(&mut server);
}

#[test]
fn detached_bye_has_a_distinct_exit() {
    let (mut client, _) = socket_pair();
    let mut state = state_with_pane();

    assert_eq!(
        apply_server_message(
            ServerMsg::Bye {
                reason: "detached".into(),
            },
            &mut state,
            &mut client,
            Size::new(80, 24),
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
        )
        .expect("Bye must apply"),
        LoopControl::Exit
    );
}

#[test]
fn peek_banner_is_fixed_above_the_tree() {
    let mut terminal = Terminal::new(TestBackend::new(40, 5)).expect("terminal must start");
    let mut state = state_with_pane();
    set_peek_person(Some("alice"));

    terminal
        .draw(|frame| draw(frame, &mut state))
        .expect("frame must draw");

    let buffer = terminal.backend().buffer();
    let first_line: String = (0..40).map(|x| buffer[(x, 0)].symbol()).collect();
    let second_line: String = (0..40).map(|x| buffer[(x, 1)].symbol()).collect();
    assert_eq!(first_line.trim_end(), "PEEK: alice - READ ONLY");
    assert_eq!(second_line.trim_end(), "Workspace: alice/w1");
    set_peek_person(None);
}

fn decode(stream: &mut TcpStream) -> ClientMsg {
    codec::decode(stream).expect("client message must decode")
}

fn assert_no_message(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .expect("read timeout must be set");
    let mut byte = [0];
    let error = stream
        .read_exact(&mut byte)
        .expect_err("event must not send a message");
    assert!(matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    ));
}

fn socket_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let client = TcpStream::connect(
        listener
            .local_addr()
            .expect("listener must have an address"),
    )
    .expect("client must connect");
    let (server, _) = listener.accept().expect("server must accept client");
    server
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("read timeout must be set");
    (client, server)
}

fn state_with_pane() -> ClientState {
    let mut tree = Tree::new();
    tree.create_workspace("main")
        .expect("workspace must be created");
    tree.create_tab("w1", "shell", PaneSize { cols: 80, rows: 24 })
        .expect("tab must be created");
    ClientState::new(tree)
}

fn state_with_two_tabs() -> ClientState {
    let mut tree = Tree::new();
    tree.create_workspace("main")
        .expect("workspace must be created");
    for title in ["first", "second"] {
        tree.create_tab("w1", title, PaneSize { cols: 80, rows: 24 })
            .expect("tab must be created");
    }
    crate::tui_navigation::initialize(&tree);
    ClientState::new(tree)
}

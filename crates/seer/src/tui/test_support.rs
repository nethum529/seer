use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Size;
use ratatui::style::Modifier;
use seer_core::proto::{ClientMsg, PeekTarget, Person, PersonState, ServerMsg, codec};
use seer_core::{
    InputEvent, KeyCode as CoreKeyCode, KeyInput, Modifiers, PaneSize, TerminalInput, Tree,
};

use super::{apply_server_message, draw, handle_event};
use crate::drawer::Drawer;
use crate::state::ClientState;

pub(super) fn decode(stream: &mut TcpStream) -> ClientMsg {
    codec::decode(stream).expect("client message must decode")
}

pub(super) fn assert_no_message(stream: &mut TcpStream) {
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

pub(crate) fn socket_pair() -> (TcpStream, TcpStream) {
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

pub(super) fn send_test_key(
    client: &mut TcpStream,
    state: &mut ClientState,
    command_pending: &mut bool,
    code: KeyCode,
    drawer: &mut Drawer,
) {
    handle_event(
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
        client,
        state,
        command_pending,
        Size::new(120, 40),
        drawer,
    )
    .expect("key must be handled");
}

pub(super) fn send_prefixed_key(
    client: &mut TcpStream,
    state: &mut ClientState,
    command_pending: &mut bool,
    code: KeyCode,
    drawer: &mut Drawer,
) {
    handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
        client,
        state,
        command_pending,
        Size::new(120, 40),
        drawer,
    )
    .expect("command prefix must be handled");
    send_test_key(client, state, command_pending, code, drawer);
}

pub(super) fn assert_key_input(stream: &mut TcpStream, pane: &str, character: char) {
    assert_eq!(
        decode(stream),
        ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: pane.into(),
            input: TerminalInput::new(InputEvent::Key(KeyInput {
                code: CoreKeyCode::Char(character),
                modifiers: Modifiers::default(),
            })),
        }
    );
}

pub(super) fn state_with_pane() -> ClientState {
    let mut tree = Tree::new();
    tree.create_workspace("main")
        .expect("workspace must be created");
    tree.create_tab("w1", "shell", PaneSize { cols: 80, rows: 24 })
        .expect("tab must be created");
    ClientState::new(tree, "alice".into())
}

pub(super) fn tree_with_two_tabs() -> Tree {
    let mut tree = Tree::new();
    tree.create_workspace("main")
        .expect("workspace must be created");
    for title in ["first", "second"] {
        tree.create_tab("w1", title, PaneSize { cols: 80, rows: 24 })
            .expect("tab must be created");
    }
    crate::tui_navigation::initialize(&tree);
    tree
}

pub(super) fn apply_two_people(
    client: &mut TcpStream,
    state: &mut ClientState,
    drawer: &mut Drawer,
) {
    let people = ["alice", "bob"]
        .into_iter()
        .map(|name| person(name, "nvim"))
        .collect();
    apply_people(client, state, drawer, people);
}

pub(super) fn apply_own_foreground(
    client: &mut TcpStream,
    state: &mut ClientState,
    drawer: &mut Drawer,
    foreground: &str,
) {
    apply_people(client, state, drawer, vec![person("alice", foreground)]);
}

fn person(name: &str, foreground: &str) -> Person {
    Person {
        user_id: name.into(),
        name: name.into(),
        attached_clients: 1,
        peekable: true,
        state: PersonState::Active,
        tabs: 1,
        foreground: foreground.into(),
        idle_secs: 12,
    }
}

fn apply_people(
    client: &mut TcpStream,
    state: &mut ClientState,
    drawer: &mut Drawer,
    people: Vec<Person>,
) {
    apply_server_message(
        ServerMsg::People { people },
        state,
        client,
        Size::new(80, 24),
        drawer,
    )
    .expect("People must apply");
}

pub(super) fn highlighted_person(drawer: &Drawer, state: &mut ClientState) -> Option<u16> {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal must start");
    terminal
        .draw(|frame| draw(frame, state, drawer))
        .expect("drawer must draw");
    let buffer = terminal.backend().buffer();
    (1..23)
        .find(|row| buffer[(50, *row)].modifier.contains(Modifier::REVERSED))
        .map(|row| row - 1)
}

pub(super) fn press_key(
    client: &mut TcpStream,
    state: &mut ClientState,
    drawer: &mut Drawer,
    code: KeyCode,
) {
    send_test_key(client, state, &mut false, code, drawer);
}

pub(super) fn apply_active_target(
    client: &mut TcpStream,
    state: &mut ClientState,
    drawer: &mut Drawer,
) {
    let targets = vec![PeekTarget {
        workspace: "w1".into(),
        workspace_name: "main".into(),
        tab: "w1:t1".into(),
        tab_title: "shell".into(),
        active: true,
    }];
    apply_server_message(
        ServerMsg::Targets { targets },
        state,
        client,
        Size::new(80, 24),
        drawer,
    )
    .expect("Targets must apply");
}

pub(super) fn peek_message() -> ClientMsg {
    ClientMsg::Peek {
        user: "alice".into(),
        workspace: "w1".into(),
        tab: "w1:t1".into(),
    }
}

pub(super) fn peek_banner_shown(state: &mut ClientState, drawer: &Drawer) -> bool {
    let mut terminal = Terminal::new(TestBackend::new(80, 5)).expect("terminal must start");
    terminal
        .draw(|frame| draw(frame, state, drawer))
        .expect("frame must draw");
    let buffer = terminal.backend().buffer();
    let line: String = (0..79).map(|x| buffer[(x, 0)].symbol()).collect();
    line.starts_with("PEEK: alice")
}

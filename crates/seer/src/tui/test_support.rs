use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Size;
use seer_core::proto::{ClientMsg, codec};
use seer_core::{
    InputEvent, KeyCode as CoreKeyCode, KeyInput, Modifiers, PaneSize, TerminalInput, Tree,
};

use super::handle_event;
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

pub(super) fn socket_pair() -> (TcpStream, TcpStream) {
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
    ClientState::new(tree)
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

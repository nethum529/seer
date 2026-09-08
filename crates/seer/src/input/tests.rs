use super::*;
use crate::panels::Panel;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use seer_core::Tree;
use seer_core::proto::{ClientMsg, TerminalInfo, codec};
use std::os::unix::net::UnixStream;
use std::time::Duration;

fn draw(terminal: &mut Terminal<TestBackend>, state: &mut ClientState) -> String {
    terminal
        .draw(|frame| crate::render::draw(frame, state))
        .expect("screen must draw");
    let buffer = terminal.backend().buffer();
    buffer.content.iter().map(|cell| cell.symbol()).collect()
}

fn click(state: &mut ClientState, stream: &mut UnixStream, area: Rect) {
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: area.x + area.width / 2,
        row: area.y + area.height / 2,
        modifiers: KeyModifiers::NONE,
    };
    mouse(event, stream, state, &mut None).expect("click must work");
}

fn press(state: &mut ClientState, stream: &mut UnixStream, code: CrosstermKeyCode) {
    command(KeyEvent::new(code, KeyModifiers::NONE), stream, state).expect("key must work");
}

fn quiet(peer: &mut UnixStream) -> bool {
    let mut byte = [0];
    std::io::Read::read(peer, &mut byte).is_err()
}

fn one_terminal_state() -> (ClientState, String) {
    let mut tree = Tree::new();
    let workspace = tree.create_workspace("main").expect("workspace must open");
    let pane = tree
        .create_tab(
            &workspace.id,
            "one",
            seer_core::PaneSize { cols: 80, rows: 24 },
        )
        .expect("tab must open")
        .panes[0]
        .id
        .clone();
    let mut state = ClientState::new(tree, "alice".into());
    state.frames.insert(
        ("alice".into(), pane.clone()),
        seer_core::TerminalFrame {
            rows: vec![
                vec![
                    seer_core::Cell {
                        character: 'x',
                        fg: seer_core::Color::Default,
                        bg: seer_core::Color::Default,
                        bold: false,
                        italic: false,
                        underline: false,
                        dim: false,
                        inverse: false,
                        hidden: false,
                        strikeout: false,
                    };
                    100
                ];
                30
            ],
            cursor: Default::default(),
            modes: Default::default(),
        },
    );
    state.terminals.insert(
        "alice".into(),
        vec![TerminalInfo {
            last_typist: None,
            pane: pane.clone(),
            name: "shell".into(),
            state: "idle".into(),
            cols: 80,
            rows: 24,
        }],
    );
    (state, pane)
}

#[test]
fn the_people_panel_opens_and_closes_by_key_click_and_escape() {
    let (mut state, _pane) = one_terminal_state();
    let (mut stream, _peer) = UnixStream::pair().expect("streams must open");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");

    assert!(
        !draw(&mut terminal, &mut state).contains("people"),
        "no people column may take content space by default"
    );
    press(&mut state, &mut stream, CrosstermKeyCode::Char('p'));
    assert!(draw(&mut terminal, &mut state).contains("people"));
    assert!(matches!(state.chrome.panel, Some(Panel::People)));

    press(&mut state, &mut stream, CrosstermKeyCode::Esc);
    assert!(state.chrome.panel.is_none(), "esc must close the panel");
    assert!(!state.quit_prompt, "esc must close before asking to quit");
    draw(&mut terminal, &mut state);

    let handle = state.chrome.handle_area;
    assert!(handle.width >= 1 && handle.height == 3, "{handle:?}");
    click(&mut state, &mut stream, handle);
    draw(&mut terminal, &mut state);
    assert!(matches!(state.chrome.panel, Some(Panel::People)));

    let chip = state.chrome.chip_area;
    click(&mut state, &mut stream, chip);
    draw(&mut terminal, &mut state);
    assert!(matches!(state.chrome.panel, Some(Panel::Session)));
    let handle = state.chrome.handle_area;
    click(&mut state, &mut stream, handle);
    draw(&mut terminal, &mut state);
    assert!(matches!(state.chrome.panel, Some(Panel::People)));

    click(&mut state, &mut stream, Rect::new(90, 25, 1, 1));
    assert!(
        state.chrome.panel.is_none(),
        "a click outside must close the panel"
    );
    assert!(state.selection.is_none(), "the click must not start a drag");

    terminal.backend_mut().resize(45, 20);
    draw(&mut terminal, &mut state);
    press(&mut state, &mut stream, CrosstermKeyCode::Char('p'));
    assert!(
        draw(&mut terminal, &mut state).contains("people"),
        "narrow screens keep the panel"
    );
    let handle = state.chrome.close_area;
    click(&mut state, &mut stream, handle);
    assert!(state.chrome.panel.is_none());
}

#[test]
fn the_viewer_forwards_keys_until_a_panel_takes_them() {
    let (mut state, pane) = one_terminal_state();
    let (mut stream, mut peer) = UnixStream::pair().expect("streams must open");
    peer.set_read_timeout(Some(Duration::from_millis(50)))
        .expect("timeout must apply");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    state.open_focused();
    draw(&mut terminal, &mut state);

    press(&mut state, &mut stream, CrosstermKeyCode::Char('p'));
    let sent = codec::decode::<_, ClientMsg>(&mut peer).expect("input must reach the terminal");
    let ClientMsg::TerminalInput {
        pane: target,
        input,
        ..
    } = sent
    else {
        panic!("p must send terminal input, not chrome, got {sent:?}");
    };
    assert_eq!(target, pane);
    assert_eq!(
        input,
        key_to_input(KeyEvent::new(
            CrosstermKeyCode::Char('p'),
            KeyModifiers::NONE
        ))
        .expect("p must map to input")
    );

    let handle = state.chrome.handle_area;
    click(&mut state, &mut stream, handle);
    draw(&mut terminal, &mut state);
    assert!(matches!(state.chrome.panel, Some(Panel::People)));
    assert!(state.viewer.is_some(), "the viewer must stay open");

    press(&mut state, &mut stream, CrosstermKeyCode::Char('z'));
    assert!(
        quiet(&mut peer),
        "an open panel must keep keys off the child"
    );
    crate::viewer::input_message(
        &mut stream,
        &state,
        seer_core::TerminalInput::new(seer_core::InputEvent::Paste("hello".into())),
    )
    .expect("paste must be handled");
    assert!(
        quiet(&mut peer),
        "an open panel must keep a paste off the child"
    );

    press(&mut state, &mut stream, CrosstermKeyCode::Esc);
    assert!(state.chrome.panel.is_none());
    assert!(state.viewer.is_some(), "esc must close only the panel");
    press(&mut state, &mut stream, CrosstermKeyCode::Char('p'));
    assert!(
        codec::decode::<_, ClientMsg>(&mut peer).is_ok(),
        "a closed panel gives the keys back to the terminal"
    );

    let mut bob = state.people[0].clone();
    bob.user_id = "bob".into();
    bob.name = "Bob".into();
    state.people.push(bob);
    state
        .terminals
        .insert("bob".into(), state.terminals["alice"].clone());
    crate::panels::open(&mut state, Panel::People);
    draw(&mut terminal, &mut state);
    let row = state
        .people_areas
        .iter()
        .find(|(index, _)| *index == 1)
        .expect("Bob row")
        .1;
    mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: row.x,
            row: row.y,
            modifiers: KeyModifiers::NONE,
        },
        &mut stream,
        &mut state,
        &mut None,
    )
    .expect("person menu");
    press(&mut state, &mut stream, CrosstermKeyCode::Enter);
    assert_eq!(state.viewer.as_ref().expect("viewer").user, "bob");
    assert!(
        !state.chrome_owns_input(),
        "watch from the person menu must dismiss the people panel"
    );
}

#[test]
fn the_session_panel_reaches_the_terminal_actions() {
    let (mut state, pane) = one_terminal_state();
    let (mut stream, mut peer) = UnixStream::pair().expect("streams must open");
    peer.set_read_timeout(Some(Duration::from_millis(50)))
        .expect("timeout");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");

    press(&mut state, &mut stream, CrosstermKeyCode::Char('s'));
    let text = draw(&mut terminal, &mut state);
    assert!(matches!(state.chrome.panel, Some(Panel::Session)));
    for label in ["1 shell", "new terminal", "close terminal", "quit"] {
        assert!(text.contains(label), "session panel must offer {label}");
    }

    press(&mut state, &mut stream, CrosstermKeyCode::Enter);
    assert_eq!(
        state.viewer.as_ref().map(|viewer| viewer.pane.as_str()),
        Some(pane.as_str()),
        "enter on a terminal row must open it"
    );

    crate::panels::open(&mut state, Panel::Session);
    press(&mut state, &mut stream, CrosstermKeyCode::Char('n'));
    let workspace = state.tree.workspaces[0].id.clone();
    assert_eq!(
        codec::decode::<_, ClientMsg>(&mut peer).expect("create"),
        ClientMsg::CreateTab { workspace }
    );
    crate::panels::open(&mut state, Panel::Session);
    press(&mut state, &mut stream, CrosstermKeyCode::Char('x'));
    assert!(
        matches!(codec::decode::<_, ClientMsg>(&mut peer).expect("close"), ClientMsg::ClosePane { pane: target, .. } if target == pane)
    );
    crate::panels::open(&mut state, Panel::Session);
    assert!(
        command(
            KeyEvent::new(CrosstermKeyCode::Char('q'), KeyModifiers::NONE),
            &mut stream,
            &mut state
        )
        .expect("quit")
    );
    state.quit_prompt = true;
    press(&mut state, &mut stream, CrosstermKeyCode::Esc);
    assert!(!state.quit_prompt);
}

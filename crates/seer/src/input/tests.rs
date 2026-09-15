use super::*;
use crate::panels::Panel;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use seer_core::Tree;
use seer_core::proto::{ClientMsg, Person, PersonState, TerminalInfo, codec};
use std::os::unix::net::UnixStream;
use std::time::Duration;

pub(super) struct Wires {
    pub(super) routes: crate::routes::Routes,
    pub(super) local: UnixStream,
    pub(super) room: UnixStream,
}

impl Wires {
    pub(super) fn quiet(&mut self) -> bool {
        quiet(&mut self.local) && quiet(&mut self.room)
    }
}

pub(super) fn wires(own_user: &str) -> Wires {
    let (local, local_peer) = UnixStream::pair().expect("streams");
    let (room, room_peer) = UnixStream::pair().expect("streams");
    for peer in [&local_peer, &room_peer] {
        peer.set_read_timeout(Some(Duration::from_millis(50)))
            .expect("timeout");
    }
    Wires {
        routes: crate::routes::Routes::new(
            seer_net::Socket::from(local),
            Some(seer_net::Socket::from(room)),
            own_user.to_owned(),
        ),
        local: local_peer,
        room: room_peer,
    }
}

pub(super) fn draw(terminal: &mut Terminal<TestBackend>, state: &mut ClientState) -> String {
    terminal
        .draw(|frame| crate::render::draw(frame, state))
        .expect("screen must draw");
    let buffer = terminal.backend().buffer();
    buffer.content.iter().map(|cell| cell.symbol()).collect()
}

pub(super) fn press_at(
    state: &mut ClientState,
    stream: &mut crate::routes::Routes,
    kind: MouseEventKind,
    area: Rect,
) {
    let event = MouseEvent {
        kind,
        column: area.x + area.width / 2,
        row: area.y + area.height / 2,
        modifiers: KeyModifiers::NONE,
    };
    mouse(event, stream, state).expect("mouse must work");
}

pub(super) fn click(state: &mut ClientState, stream: &mut crate::routes::Routes, area: Rect) {
    press_at(state, stream, MouseEventKind::Down(MouseButton::Left), area);
}

pub(super) fn right_click(state: &mut ClientState, stream: &mut crate::routes::Routes, area: Rect) {
    press_at(
        state,
        stream,
        MouseEventKind::Down(MouseButton::Right),
        area,
    );
}

pub(super) fn press(
    state: &mut ClientState,
    stream: &mut crate::routes::Routes,
    code: CrosstermKeyCode,
) {
    command(KeyEvent::new(code, KeyModifiers::NONE), stream, state).expect("key must work");
}

pub(super) fn quiet(peer: &mut UnixStream) -> bool {
    let mut byte = [0];
    std::io::Read::read(peer, &mut byte).is_err()
}

pub(super) fn person(user: &str, name: &str, host: bool) -> Person {
    Person {
        user_id: user.into(),
        name: name.into(),
        online: true,
        idle_secs: 0,
        attached_clients: 1,
        peekable: true,
        host,
        state: PersonState::Active,
        tabs: 0,
        foreground: String::new(),
    }
}

pub(super) fn one_terminal_state() -> (ClientState, String) {
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

fn two_person_state() -> (ClientState, String) {
    let (mut state, pane) = one_terminal_state();
    state.note_people(&[person("alice", "Alice", true), person("bob", "Bob", false)]);
    state.server = "127.0.0.1:7321".into();
    state
        .terminals
        .insert("bob".into(), state.terminals["alice"].clone());
    (state, pane)
}

fn row_of(state: &ClientState, index: usize) -> Rect {
    state
        .people_areas
        .iter()
        .find(|(row, _)| *row == index)
        .expect("the picker must list the person")
        .1
}

#[test]
fn the_top_right_control_names_the_viewed_person_and_opens_the_picker() {
    let (mut state, _pane) = two_person_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");

    let text = draw(&mut terminal, &mut state);
    assert!(text.contains("Seer Alice"), "the control names the viewer");
    for absent in ["Your terminal", "Can type", "Read only", "people"] {
        assert!(!text.contains(absent), "the control must not show {absent}");
    }
    assert_eq!(
        state.box_areas[0].content,
        Rect::new(0, 0, 100, 30),
        "the terminal must keep the whole window"
    );

    let chip = state.chrome.chip_area;
    click(&mut state, &mut wires.routes, chip);
    let text = draw(&mut terminal, &mut state);
    assert!(matches!(state.chrome.panel, Some(Panel::Picker)));
    assert!(text.contains("Permissions granted for Alice"));
    assert!(text.contains("Alice") && text.contains("Bob"));
    assert!(text.contains("host"), "the picker must mark the host");
    assert!(
        text.contains("127.0.0.1:7321"),
        "the picker shows the server"
    );
    for absent in ["new terminal", "close terminal", "quit", "access"] {
        assert!(!text.contains(absent), "the picker must not offer {absent}");
    }

    let row = row_of(&state, 1);
    click(&mut state, &mut wires.routes, row);
    assert_eq!(state.user(), "bob", "a click selects that person");
    assert!(state.chrome.panel.is_none(), "the click closes the picker");
    let text = draw(&mut terminal, &mut state);
    assert!(text.contains("Seer Bob"), "the control follows the choice");

    let chip = state.chrome.chip_area;
    click(&mut state, &mut wires.routes, chip);
    let text = draw(&mut terminal, &mut state);
    assert!(text.contains("Permissions not granted for Bob"));
    state.you_may_type_into.insert("bob".into());
    let text = draw(&mut terminal, &mut state);
    assert!(text.contains("Permissions granted for Bob"));
}

#[test]
fn the_picker_fits_many_people_in_a_small_window() {
    let (mut state, _pane) = one_terminal_state();
    let mut wires = wires("alice");
    let people: Vec<Person> = (0..16)
        .map(|index| person(&format!("u{index}"), &format!("person{index}"), index == 0))
        .collect();
    let mut people = people;
    people[0].user_id = "alice".into();
    state.note_people(&people);
    state.server = "127.0.0.1:7321".into();

    for (width, height) in [(60, 14), (100, 30), (200, 40)] {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("backend must open");
        crate::panels::open(&mut state, Panel::Picker);
        draw(&mut terminal, &mut state);
        let area = state.chrome.panel_area;
        assert!(
            area.right() <= width && area.bottom() <= height && !area.is_empty(),
            "the picker must fit {width}x{height}, got {area:?}"
        );
        assert!(
            !state.people_areas.is_empty(),
            "the picker must offer rows at {width}x{height}"
        );
        for (_, row) in &state.people_areas {
            assert!(
                area.contains(ratatui::layout::Position::new(row.x, row.y)),
                "every row must stay inside the picker"
            );
        }
        let row = state.people_areas[0].1;
        click(&mut state, &mut wires.routes, row);
        assert!(state.chrome.panel.is_none());
    }
}

#[test]
fn the_session_actions_stay_on_the_right_click_of_the_control() {
    let (mut state, pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");

    draw(&mut terminal, &mut state);
    let chip = state.chrome.chip_area;
    right_click(&mut state, &mut wires.routes, chip);
    let text = draw(&mut terminal, &mut state);
    assert!(matches!(state.chrome.panel, Some(Panel::Session)));
    for label in ["1 shell", "new terminal", "close terminal", "quit"] {
        assert!(text.contains(label), "session panel must offer {label}");
    }

    press(&mut state, &mut wires.routes, CrosstermKeyCode::Enter);
    assert_eq!(
        state.viewer.as_ref().map(|viewer| viewer.pane.as_str()),
        Some(pane.as_str()),
        "enter on a terminal row must open it"
    );

    crate::panels::open(&mut state, Panel::Session);
    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('n'));
    let workspace = state.tree.workspaces[0].id.clone();
    assert_eq!(
        codec::decode::<_, ClientMsg>(&mut wires.local).expect("create"),
        ClientMsg::CreateTab { workspace }
    );
    crate::panels::open(&mut state, Panel::Session);
    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('x'));
    assert!(
        matches!(codec::decode::<_, ClientMsg>(&mut wires.local).expect("close"), ClientMsg::ClosePane { pane: target, .. } if target == pane)
    );
    crate::panels::open(&mut state, Panel::Session);
    assert!(
        command(
            KeyEvent::new(CrosstermKeyCode::Char('q'), KeyModifiers::NONE),
            &mut wires.routes,
            &mut state
        )
        .expect("quit")
    );
}

#[test]
fn the_viewer_forwards_keys_until_a_panel_takes_them() {
    let (mut state, pane) = two_person_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    state.open_focused();
    draw(&mut terminal, &mut state);

    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('p'));
    let sent =
        codec::decode::<_, ClientMsg>(&mut wires.local).expect("input must reach the terminal");
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

    let chip = state.chrome.chip_area;
    click(&mut state, &mut wires.routes, chip);
    draw(&mut terminal, &mut state);
    assert!(matches!(state.chrome.panel, Some(Panel::Picker)));
    assert!(state.viewer.is_some(), "the viewer must stay open");

    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('z'));
    assert!(wires.quiet(), "an open panel must keep keys off the child");
    crate::viewer::input_message(
        &mut wires.routes,
        &mut state,
        seer_core::TerminalInput::new(seer_core::InputEvent::Paste("hello".into())),
    )
    .expect("paste must be handled");
    assert!(
        wires.quiet(),
        "an open panel must keep a paste off the child"
    );

    press(&mut state, &mut wires.routes, CrosstermKeyCode::Esc);
    assert!(state.chrome.panel.is_none(), "esc must close the picker");
    assert!(state.viewer.is_some(), "esc must close only the panel");
    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('p'));
    assert!(
        codec::decode::<_, ClientMsg>(&mut wires.local).is_ok(),
        "a closed panel gives the keys back to the terminal"
    );

    let chip = state.chrome.chip_area;
    click(&mut state, &mut wires.routes, chip);
    draw(&mut terminal, &mut state);
    let row = row_of(&state, 1);
    right_click(&mut state, &mut wires.routes, row);
    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char(' '));
    assert_eq!(
        codec::decode::<_, ClientMsg>(&mut wires.room).expect("grant"),
        ClientMsg::SetGrant {
            user: "bob".into(),
            can_type: true
        },
        "the person menu must still toggle the grant"
    );
    press(&mut state, &mut wires.routes, CrosstermKeyCode::Enter);
    assert_eq!(state.viewer.as_ref().expect("viewer").user, "bob");
    assert!(
        !state.chrome_owns_input(),
        "watch from the person menu must dismiss the picker"
    );
}

pub(super) fn click_release(
    state: &mut ClientState,
    stream: &mut crate::routes::Routes,
    area: Rect,
) {
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        press_at(state, stream, kind, area);
    }
}

#[test]
fn a_right_click_on_a_person_keeps_the_current_view() {
    let (mut state, _pane) = two_person_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    state.open_focused();
    draw(&mut terminal, &mut state);

    let chip = state.chrome.chip_area;
    click(&mut state, &mut wires.routes, chip);
    draw(&mut terminal, &mut state);
    let row = row_of(&state, 1);
    right_click(&mut state, &mut wires.routes, row);
    assert!(state.menu.is_some(), "a right click opens the person menu");
    assert_eq!(state.user(), "alice", "a right click must not select bob");
    assert_eq!(
        state.viewer.as_ref().map(|viewer| viewer.user.as_str()),
        Some("alice"),
        "a right click must keep the open viewer"
    );

    press(&mut state, &mut wires.routes, CrosstermKeyCode::Enter);
    assert_eq!(state.user(), "bob", "watch from the menu selects bob");
    assert_eq!(state.viewer.as_ref().expect("viewer").user, "bob");
}

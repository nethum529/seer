use super::tests::{click, click_release, draw, one_terminal_state, press, quiet, wires};
use super::*;
use crate::panels::Panel;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use seer_core::proto::{ClientMsg, codec};
use std::os::unix::net::UnixStream;

fn at(
    state: &mut ClientState,
    stream: &mut crate::routes::Routes,
    kind: MouseEventKind,
    area: Rect,
) {
    mouse(
        MouseEvent {
            kind,
            column: area.x + area.width / 2,
            row: area.y + area.height / 2,
            modifiers: KeyModifiers::NONE,
        },
        stream,
        state,
        &mut None,
    )
    .expect("mouse must work");
}

#[test]
fn a_panel_action_does_not_click_through_to_the_terminal_behind_it() {
    let (mut state, _pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    state.open_focused();
    draw(&mut terminal, &mut state);

    let chip = state.chrome.chip_area;
    click(&mut state, &mut wires.routes, chip);
    draw(&mut terminal, &mut state);
    let back = state
        .chrome
        .rows
        .iter()
        .find(|(index, _)| crate::panels::rows(&state)[*index] == crate::panels::Row::Back)
        .expect("back row")
        .1;
    at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Down(MouseButton::Left),
        back,
    );
    assert!(state.viewer.is_none(), "back must leave the viewer");
    draw(&mut terminal, &mut state);
    at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Up(MouseButton::Left),
        back,
    );
    assert!(
        state.viewer.is_none(),
        "releasing the same click must not reopen the terminal"
    );

    crate::panels::open(&mut state, Panel::People);
    draw(&mut terminal, &mut state);
    let row = state.people_areas[0].1;
    at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Down(MouseButton::Left),
        row,
    );
    draw(&mut terminal, &mut state);
    at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Up(MouseButton::Left),
        row,
    );
    assert!(
        state.viewer.is_none(),
        "a person row must not open a terminal behind the panel"
    );
}

#[test]
fn the_search_field_takes_keys_and_gives_them_back_to_the_terminal() {
    let (mut state, pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    crate::panels::open(&mut state, Panel::People);
    draw(&mut terminal, &mut state);

    let field = state.chrome.search_area;
    assert!(
        field.width > 1,
        "the people panel must offer a search field"
    );
    click_release(&mut state, &mut wires.routes, field);
    assert!(state.searching, "a click on the field must start a search");
    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('n'));
    assert_eq!(state.search, "n", "typing must reach the field");
    assert!(
        quiet(&mut wires.local),
        "the field must not leak to a terminal"
    );

    draw(&mut terminal, &mut state);
    let tile = state.box_areas[0].content;
    click_release(&mut state, &mut wires.routes, tile);
    assert!(
        state.chrome.panel.is_none(),
        "the click must close the panel"
    );
    assert!(!state.searching, "the click must leave the search field");
    assert!(
        state.viewer.is_some(),
        "one click on content must dismiss the overlay and take typing"
    );
    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('n'));
    assert!(
        matches!(
            codec::decode::<_, ClientMsg>(&mut wires.local).expect("input"),
            ClientMsg::TerminalInput { pane: target, .. } if target == pane
        ),
        "keys must go back to the terminal"
    );

    state.chrome.pinned = true;
    draw(&mut terminal, &mut state);
    let field = state.chrome.search_area;
    click_release(&mut state, &mut wires.routes, field);
    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('x'));
    assert_eq!(state.search, "x", "pinned search must take its own input");
    crate::viewer::input_message(
        &mut wires.routes,
        &mut state,
        TerminalInput::new(InputEvent::Paste("query".into())),
    )
    .expect("paste must be handled");
    assert!(
        quiet(&mut wires.local),
        "pinned search must not leak keys or paste"
    );
    let content = state.viewer.as_ref().expect("viewer must stay open").area;
    click_release(&mut state, &mut wires.routes, content);
    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('n'));
    assert!(
        matches!(
            codec::decode::<_, ClientMsg>(&mut wires.local).expect("input"),
            ClientMsg::TerminalInput { pane: target, .. } if target == pane
        ),
        "clicking content must restore typing while keeping the sidebar pinned"
    );
    assert!(state.chrome.pinned);
}

#[test]
fn a_drag_selects_text_and_does_not_open_the_terminal() {
    let (mut state, _pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    draw(&mut terminal, &mut state);
    let tile = state.box_areas[0].content;

    at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Down(MouseButton::Left),
        tile,
    );
    at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Drag(MouseButton::Left),
        Rect::new(tile.x + 4, tile.y, 1, 1),
    );
    at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Up(MouseButton::Left),
        tile,
    );
    assert_eq!(state.notice, "Copied", "a drag must still copy");
    assert!(
        state.viewer.is_none(),
        "a drag must not open the terminal it selected in"
    );
}

fn typed_bytes(peer: &mut UnixStream, pane: &str) -> Vec<u8> {
    match codec::decode::<_, ClientMsg>(peer).expect("input must reach the terminal") {
        ClientMsg::TypeInto {
            user,
            pane: target,
            bytes,
        } => {
            assert_eq!(user, "bob", "input must go to the watched person");
            assert_eq!(target, pane);
            bytes
        }
        other => panic!("expected remote terminal input, got {other:?}"),
    }
}

#[test]
fn one_click_gives_typing_to_a_granted_remote_terminal() {
    let (mut state, pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut bob = state.people[0].clone();
    bob.user_id = "bob".into();
    bob.name = "Bob".into();
    state.people.push(bob);
    state
        .terminals
        .insert("bob".into(), state.terminals["alice"].clone());
    state.frames.insert(
        ("bob".into(), pane.clone()),
        state.frames[&("alice".into(), pane.clone())].clone(),
    );
    state.you_may_type_into.insert("bob".into());
    state.selected = 1;
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    draw(&mut terminal, &mut state);

    let tile = state.box_areas[0].content;
    click_release(&mut state, &mut wires.routes, tile);
    assert_eq!(
        state.viewer.as_ref().map(|viewer| viewer.user.as_str()),
        Some("bob"),
        "one click on terminal content must take typing"
    );

    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('n'));
    assert_eq!(
        typed_bytes(&mut wires.room, &pane),
        b"n",
        "a letter must reach the terminal, not create one"
    );

    command(
        KeyEvent::new(CrosstermKeyCode::Char('b'), KeyModifiers::CONTROL),
        &mut wires.routes,
        &mut state,
    )
    .expect("ctrl+b must work");
    assert_eq!(
        typed_bytes(&mut wires.room, &pane),
        b"\x02",
        "ctrl+b must reach the terminal"
    );
    assert!(state.viewer.is_some(), "ctrl+b must not leave the terminal");

    state.you_may_type_into.remove("bob");
    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('n'));
    assert!(quiet(&mut wires.local), "a revoked grant must stop input");
}

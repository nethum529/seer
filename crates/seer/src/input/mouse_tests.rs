use super::tests::{click_release, draw, one_terminal_state, press, press_at, right_click, wires};
use super::*;
use crate::panels::Panel;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use seer_core::proto::{ClientMsg, codec};
use std::os::unix::net::UnixStream;

#[test]
fn a_panel_action_does_not_click_through_to_the_terminal_behind_it() {
    let (mut state, _pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    state.open_focused();
    draw(&mut terminal, &mut state);

    let chip = state.chrome.chip_area;
    right_click(&mut state, &mut wires.routes, chip);
    draw(&mut terminal, &mut state);
    let back = state
        .chrome
        .rows
        .iter()
        .find(|(index, _)| crate::panels::rows(&state)[*index] == crate::panels::Row::Back)
        .expect("back row")
        .1;
    press_at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Down(MouseButton::Left),
        back,
    );
    assert!(state.viewer.is_none(), "back must leave the viewer");
    draw(&mut terminal, &mut state);
    press_at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Up(MouseButton::Left),
        back,
    );
    assert!(
        state.viewer.is_none(),
        "releasing the same click must not reopen the terminal"
    );

    crate::panels::open(&mut state, Panel::Picker);
    draw(&mut terminal, &mut state);
    let row = state.people_areas[0].1;
    press_at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Down(MouseButton::Left),
        row,
    );
    draw(&mut terminal, &mut state);
    press_at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Up(MouseButton::Left),
        row,
    );
    assert!(
        state.viewer.is_none(),
        "a person row must not open a terminal behind the picker"
    );
}

#[test]
fn a_click_outside_the_picker_closes_it_and_gives_typing_back() {
    let (mut state, pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    crate::panels::open(&mut state, Panel::Picker);
    draw(&mut terminal, &mut state);

    press(&mut state, &mut wires.routes, CrosstermKeyCode::Char('n'));
    assert!(wires.quiet(), "the picker must not leak keys");

    let tile = state.box_areas[0].content;
    click_release(&mut state, &mut wires.routes, tile);
    assert!(
        state.chrome.panel.is_none(),
        "the click must close the picker"
    );
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
}

#[test]
fn a_drag_selects_text_and_does_not_open_the_terminal() {
    let (mut state, _pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    draw(&mut terminal, &mut state);
    let tile = state.box_areas[0].content;

    press_at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Down(MouseButton::Left),
        tile,
    );
    press_at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Drag(MouseButton::Left),
        Rect::new(tile.x + 4, tile.y, 1, 1),
    );
    press_at(
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
    assert!(wires.quiet(), "a revoked grant must stop input");
}

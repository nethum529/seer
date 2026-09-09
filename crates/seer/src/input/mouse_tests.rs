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

#[test]
fn a_click_reaches_a_terminal_program_that_asked_for_the_mouse() {
    let (mut state, pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    state
        .frames
        .get_mut(&("alice".into(), pane.clone()))
        .expect("frame")
        .modes
        .mouse_tracking = seer_core::MouseTracking::Click;
    state.open_focused();
    draw(&mut terminal, &mut state);
    let area = state.viewer.as_ref().expect("viewer").area;

    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: area.x + 5,
        row: area.y + 2,
        modifiers: KeyModifiers::NONE,
    };
    mouse(event, &mut wires.routes, &mut state).expect("mouse must work");

    match codec::decode::<_, ClientMsg>(&mut wires.local).expect("the click must reach the program")
    {
        ClientMsg::TerminalInput {
            pane: target,
            input,
            ..
        } => {
            assert_eq!(target, pane);
            assert_eq!(
                input.event,
                seer_core::InputEvent::Mouse(seer_core::MouseInput {
                    kind: seer_core::MouseKind::Down,
                    button: Some(seer_core::MouseButton::Left),
                    column: 5,
                    row: 2,
                    modifiers: Modifiers::default(),
                }),
                "the program must get the click at its own cell"
            );
        }
        other => panic!("expected terminal input, got {other:?}"),
    }
    assert!(
        state.selection.is_none(),
        "the program owns the click, so no selection starts"
    );
}

#[test]
fn the_top_right_control_keeps_its_click_when_the_program_asked_for_the_mouse() {
    let (mut state, pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    state
        .frames
        .get_mut(&("alice".into(), pane))
        .expect("frame")
        .modes
        .mouse_tracking = seer_core::MouseTracking::Click;
    state.open_focused();
    draw(&mut terminal, &mut state);

    let chip = state.chrome.chip_area;
    press_at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Down(MouseButton::Left),
        chip,
    );
    assert_eq!(
        state.chrome.panel,
        Some(Panel::Picker),
        "the control must still open the picker"
    );
    assert!(
        wires.quiet(),
        "the control click must not reach the program"
    );
}

#[test]
fn a_chrome_action_that_opens_a_terminal_does_not_leak_its_release() {
    let (mut state, pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    state
        .frames
        .get_mut(&("alice".into(), pane))
        .expect("frame")
        .modes
        .mouse_tracking = seer_core::MouseTracking::Click;
    crate::panels::open(&mut state, Panel::Session);
    draw(&mut terminal, &mut state);
    let row = state
        .chrome
        .rows
        .iter()
        .find(|(index, _)| crate::panels::rows(&state)[*index] == crate::panels::Row::Terminal(0))
        .expect("terminal row")
        .1;

    press_at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Down(MouseButton::Left),
        row,
    );
    draw(&mut terminal, &mut state);
    assert!(state.viewer.is_some(), "the row must open the terminal");
    let area = state.viewer.as_ref().expect("viewer").area;
    press_at(
        &mut state,
        &mut wires.routes,
        MouseEventKind::Up(MouseButton::Left),
        area,
    );
    assert!(
        wires.quiet(),
        "a release from a chrome press must not reach the program"
    );
}

#[test]
fn the_first_click_after_attach_reaches_a_program_that_asked_for_the_mouse() {
    let (mut state, pane) = one_terminal_state();
    let mut wires = wires("alice");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    state
        .frames
        .get_mut(&("alice".into(), pane.clone()))
        .expect("frame")
        .modes
        .mouse_tracking = seer_core::MouseTracking::Click;
    draw(&mut terminal, &mut state);
    assert!(state.viewer.is_none(), "attach starts in the overview");
    let tile = state.box_areas[0].content;

    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: tile.x + 3,
        row: tile.y + 1,
        modifiers: KeyModifiers::NONE,
    };
    mouse(event, &mut wires.routes, &mut state).expect("mouse must work");

    assert!(state.viewer.is_some(), "the click must open the terminal");
    match codec::decode::<_, ClientMsg>(&mut wires.local).expect("the click must reach the program")
    {
        ClientMsg::TerminalInput {
            pane: target,
            input,
            ..
        } => {
            assert_eq!(target, pane);
            assert_eq!(
                input.event,
                seer_core::InputEvent::Mouse(seer_core::MouseInput {
                    kind: seer_core::MouseKind::Down,
                    button: Some(seer_core::MouseButton::Left),
                    column: 3,
                    row: 1,
                    modifiers: Modifiers::default(),
                }),
                "the first click must land on the cell the user saw"
            );
        }
        other => panic!("expected terminal input, got {other:?}"),
    }
}

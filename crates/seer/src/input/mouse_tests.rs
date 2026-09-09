use super::tests::{click, click_release, draw, one_terminal_state, press, quiet};
use super::*;
use crate::panels::Panel;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use seer_core::proto::{ClientMsg, codec};
use std::os::unix::net::UnixStream;
use std::time::Duration;

fn at(state: &mut ClientState, stream: &mut UnixStream, kind: MouseEventKind, area: Rect) {
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
    let (mut stream, _peer) = UnixStream::pair().expect("streams must open");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    state.open_focused();
    draw(&mut terminal, &mut state);

    let chip = state.chrome.chip_area;
    click(&mut state, &mut stream, chip);
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
        &mut stream,
        MouseEventKind::Down(MouseButton::Left),
        back,
    );
    assert!(state.viewer.is_none(), "back must leave the viewer");
    draw(&mut terminal, &mut state);
    at(
        &mut state,
        &mut stream,
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
        &mut stream,
        MouseEventKind::Down(MouseButton::Left),
        row,
    );
    draw(&mut terminal, &mut state);
    at(
        &mut state,
        &mut stream,
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
    let (mut stream, mut peer) = UnixStream::pair().expect("streams must open");
    peer.set_read_timeout(Some(Duration::from_millis(50)))
        .expect("timeout must apply");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    crate::panels::open(&mut state, Panel::People);
    draw(&mut terminal, &mut state);

    let field = state.chrome.search_area;
    assert!(
        field.width > 1,
        "the people panel must offer a search field"
    );
    click_release(&mut state, &mut stream, field);
    assert!(state.searching, "a click on the field must start a search");
    press(&mut state, &mut stream, CrosstermKeyCode::Char('n'));
    assert_eq!(state.search, "n", "typing must reach the field");
    assert!(quiet(&mut peer), "the field must not leak to a terminal");

    draw(&mut terminal, &mut state);
    let tile = state.box_areas[0].content;
    click_release(&mut state, &mut stream, tile);
    assert!(
        state.chrome.panel.is_none(),
        "the click must close the panel"
    );
    assert!(!state.searching, "the click must leave the search field");
    assert!(
        state.viewer.is_some(),
        "one click on content must dismiss the overlay and take typing"
    );
    press(&mut state, &mut stream, CrosstermKeyCode::Char('n'));
    assert!(
        matches!(
            codec::decode::<_, ClientMsg>(&mut peer).expect("input"),
            ClientMsg::TerminalInput { pane: target, .. } if target == pane
        ),
        "keys must go back to the terminal"
    );
}

#[test]
fn a_drag_selects_text_and_does_not_open_the_terminal() {
    let (mut state, _pane) = one_terminal_state();
    let (mut stream, _peer) = UnixStream::pair().expect("streams must open");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
    draw(&mut terminal, &mut state);
    let tile = state.box_areas[0].content;

    at(
        &mut state,
        &mut stream,
        MouseEventKind::Down(MouseButton::Left),
        tile,
    );
    at(
        &mut state,
        &mut stream,
        MouseEventKind::Drag(MouseButton::Left),
        Rect::new(tile.x + 4, tile.y, 1, 1),
    );
    at(
        &mut state,
        &mut stream,
        MouseEventKind::Up(MouseButton::Left),
        tile,
    );
    assert_eq!(state.notice, "Copied", "a drag must still copy");
    assert!(
        state.viewer.is_none(),
        "a drag must not open the terminal it selected in"
    );
}

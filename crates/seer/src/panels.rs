use crate::{state::ClientState, tui_navigation as navigation};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;
use std::io;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Panel {
    Picker,
    Session,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Row {
    Terminal(usize),
    New,
    Close,
    Back,
    CopyInvite,
    Quit,
}

pub(crate) enum Handled {
    Passed,
    Consumed,
    Quit,
}

pub(crate) fn open(state: &mut ClientState, panel: Panel) {
    state.chrome.panel = Some(panel);
    state.chrome.row = rows(state)
        .iter()
        .position(|row| *row == Row::Terminal(state.focus))
        .unwrap_or(0);
    state.selection = None;
}

pub(crate) fn close(state: &mut ClientState) {
    state.chrome.panel = None;
    state.selection = None;
}

pub(crate) fn rows(state: &ClientState) -> Vec<Row> {
    let own = state.user() == state.own_user;
    let mut rows: Vec<Row> = (0..state.selected_terminals().len())
        .map(Row::Terminal)
        .collect();
    if own {
        rows.push(Row::New);
        if state.invite.is_some() {
            rows.push(Row::CopyInvite);
        }
        if !state.selected_terminals().is_empty() {
            rows.push(Row::Close);
        }
    }
    if state.viewer.is_some() {
        rows.push(Row::Back);
    }
    rows.push(Row::Quit);
    rows
}

pub(crate) fn key(
    key: KeyEvent,
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<bool> {
    let Some(panel) = state.chrome.panel else {
        return Ok(false);
    };
    if key.modifiers.intersects(
        KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SUPER
            | KeyModifiers::HYPER
            | KeyModifiers::META,
    ) {
        return Ok(false);
    }
    match panel {
        Panel::Picker => {
            if key.code == KeyCode::Esc {
                close(state);
            }
            Ok(false)
        }
        Panel::Session => session_key(key, stream, state),
    }
}

fn session_key(
    key: KeyEvent,
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<bool> {
    let count = rows(state).len();
    match key.code {
        KeyCode::Esc => close(state),
        KeyCode::Char('j') | KeyCode::Down => {
            state.chrome.row = (state.chrome.row + 1).min(count.saturating_sub(1));
        }
        KeyCode::Char('k') | KeyCode::Up => state.chrome.row = state.chrome.row.saturating_sub(1),
        KeyCode::Enter => return activate(stream, state, state.chrome.row),
        KeyCode::Char(number @ '1'..='9') => {
            let index = number as usize - '1' as usize;
            if index < state.selected_terminals().len() {
                return activate(stream, state, index);
            }
        }
        KeyCode::Char('n') => return activate_row(stream, state, Row::New),
        KeyCode::Char('x') => return activate_row(stream, state, Row::Close),
        KeyCode::Char('c') => return activate_row(stream, state, Row::CopyInvite),
        KeyCode::Char('q') => return activate_row(stream, state, Row::Quit),
        _ => {}
    }
    Ok(false)
}

fn activate_row(
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
    row: Row,
) -> io::Result<bool> {
    if let Some(index) = rows(state).iter().position(|item| *item == row) {
        return activate(stream, state, index);
    }
    Ok(false)
}

fn activate(
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
    index: usize,
) -> io::Result<bool> {
    let Some(row) = rows(state).get(index).copied() else {
        return Ok(false);
    };
    match row {
        Row::Terminal(terminal) => {
            state.select_tab(terminal);
            state.open_focused();
        }
        Row::New => navigation::new_terminal(stream, state)?,
        Row::Close => state.request_close(stream)?,
        Row::Back => state.viewer = None,
        Row::CopyInvite => navigation::copy_invite(state)?,
        Row::Quit => return Ok(true),
    }
    close(state);
    Ok(false)
}

pub(crate) fn mouse(
    mouse: MouseEvent,
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<Handled> {
    let position = Position::new(mouse.column, mouse.row);
    let Some(panel) = state.chrome.panel else {
        return Ok(closed_mouse(mouse, state, position));
    };
    if matches!(
        mouse.kind,
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
    ) {
        panel_scroll(mouse, state, panel);
        return Ok(Handled::Consumed);
    }
    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left | MouseButton::Right)
    ) {
        return Ok(Handled::Consumed);
    }
    if state.chrome.chip_area.contains(position) {
        if mouse.kind == MouseEventKind::Down(MouseButton::Right) {
            open(state, Panel::Session);
        } else {
            close(state);
        }
        return Ok(Handled::Consumed);
    }
    if !state.chrome.panel_area.contains(position) || state.chrome.close_area.contains(position) {
        let on_terminal = mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && !state.chrome.close_area.contains(position)
            && state
                .box_areas
                .iter()
                .any(|tile| tile.content.contains(position));
        close(state);
        if on_terminal {
            return Ok(Handled::Passed);
        }
        return Ok(Handled::Consumed);
    }
    match panel {
        Panel::Picker => Ok(picker_click(mouse, state, position)),
        Panel::Session => session_click(stream, state, position),
    }
}

fn panel_scroll(mouse: MouseEvent, state: &mut ClientState, panel: Panel) {
    let position = Position::new(mouse.column, mouse.row);
    if !state.chrome.panel_area.contains(position) {
        return;
    }
    let up = mouse.kind == MouseEventKind::ScrollUp;
    match panel {
        Panel::Picker => {
            state.picker_scroll = if up {
                state.picker_scroll.saturating_sub(1)
            } else {
                state.picker_scroll.saturating_add(1)
            };
        }
        Panel::Session => {
            state.chrome.row = if up {
                state.chrome.row.saturating_sub(1)
            } else {
                (state.chrome.row + 1).min(rows(state).len().saturating_sub(1))
            };
        }
    }
}

fn session_click(
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
    position: Position,
) -> io::Result<Handled> {
    let row = state
        .chrome
        .rows
        .iter()
        .find(|(_, area)| area.contains(position))
        .map(|(index, _)| *index);
    if let Some(index) = row {
        state.chrome.row = index;
        if activate(stream, state, index)? {
            return Ok(Handled::Quit);
        }
    }
    Ok(Handled::Consumed)
}

fn closed_mouse(mouse: MouseEvent, state: &mut ClientState, position: Position) -> Handled {
    if !state.chrome.chip_area.contains(position) {
        return Handled::Passed;
    }
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => open(state, Panel::Picker),
        MouseEventKind::Down(MouseButton::Right) => open(state, Panel::Session),
        MouseEventKind::ScrollDown
        | MouseEventKind::ScrollUp
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => return Handled::Passed,
        _ => {}
    }
    Handled::Consumed
}

fn picker_click(mouse: MouseEvent, state: &mut ClientState, position: Position) -> Handled {
    let Some((index, row)) = state
        .people_areas
        .iter()
        .find(|(_, area)| area.contains(position))
        .copied()
    else {
        return Handled::Consumed;
    };
    if mouse.kind == MouseEventKind::Down(MouseButton::Right) {
        crate::person_menu::open(state, index, row);
        return Handled::Consumed;
    }
    state.select_person(index);
    close(state);
    Handled::Consumed
}

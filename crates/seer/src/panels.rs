use crate::{state::ClientState, tui_navigation as navigation};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;
use seer_net::Stream;
use std::{io, time::Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Panel {
    People,
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
    state.searching = false;
    state.search.clear();
}

pub(crate) fn close(state: &mut ClientState) {
    state.chrome.panel = None;
    state.selection = None;
    state.searching = false;
    state.search.clear();
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
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<bool> {
    let Some(panel) = state.chrome.panel else {
        return Ok(false);
    };
    if key.code == KeyCode::Char('b') && key.modifiers == KeyModifiers::CONTROL {
        state.viewer = None;
        close(state);
        return Ok(false);
    }
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
        Panel::People => people_key(key, stream, state),
        Panel::Session => session_key(key, stream, state),
    }
}

fn people_key(
    key: KeyEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<bool> {
    if !state.searching {
        match key.code {
            KeyCode::Esc | KeyCode::Char('p') => {
                close(state);
                return Ok(false);
            }
            KeyCode::Char('s') => {
                open(state, Panel::Session);
                return Ok(false);
            }
            KeyCode::Char('m') => {
                let row = state
                    .people_areas
                    .iter()
                    .find(|(index, _)| *index == state.selected)
                    .map_or(state.chrome.panel_area, |(_, row)| *row);
                crate::person_menu::open(state, state.selected, row);
                return Ok(false);
            }
            _ => {}
        }
    }
    let selecting =
        !state.searching && matches!(key.code, KeyCode::Enter | KeyCode::Char('1'..='9'));
    let quit = navigation::key(key, stream, state)?;
    if selecting && state.viewer.is_some() {
        close(state);
    }
    Ok(quit)
}

fn session_key(
    key: KeyEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<bool> {
    let count = rows(state).len();
    match key.code {
        KeyCode::Esc | KeyCode::Char('s') => close(state),
        KeyCode::Char('p') => open(state, Panel::People),
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

fn activate_row(stream: &mut impl Stream, state: &mut ClientState, row: Row) -> io::Result<bool> {
    if let Some(index) = rows(state).iter().position(|item| *item == row) {
        return activate(stream, state, index);
    }
    Ok(false)
}

fn activate(stream: &mut impl Stream, state: &mut ClientState, index: usize) -> io::Result<bool> {
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
    stream: &mut impl Stream,
    state: &mut ClientState,
    last: &mut Option<(String, usize, Instant)>,
) -> io::Result<Handled> {
    let position = Position::new(mouse.column, mouse.row);
    let Some(panel) = state.chrome.panel else {
        return Ok(closed_mouse(mouse, state, position));
    };
    if matches!(
        mouse.kind,
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
    ) {
        if panel == Panel::People && state.chrome.panel_area.contains(position) {
            let up = mouse.kind == MouseEventKind::ScrollUp;
            state.people_scroll = if up {
                state.people_scroll.saturating_sub(1)
            } else {
                state.people_scroll.saturating_add(1)
            };
        } else if panel == Panel::Session && state.chrome.panel_area.contains(position) {
            state.chrome.row = if mouse.kind == MouseEventKind::ScrollUp {
                state.chrome.row.saturating_sub(1)
            } else {
                (state.chrome.row + 1).min(rows(state).len().saturating_sub(1))
            };
        }
        return Ok(Handled::Consumed);
    }
    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left | MouseButton::Right)
    ) {
        return Ok(Handled::Consumed);
    }
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        if panel == Panel::People && state.chrome.chip_area.contains(position) {
            open(state, Panel::Session);
            return Ok(Handled::Consumed);
        }
        if panel == Panel::Session && state.chrome.handle_area.contains(position) {
            open(state, Panel::People);
            return Ok(Handled::Consumed);
        }
    }
    if !state.chrome.panel_area.contains(position) || state.chrome.close_area.contains(position) {
        close(state);
        return Ok(Handled::Consumed);
    }
    match panel {
        Panel::People => people_click(mouse, state, position, last),
        Panel::Session => {
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
    }
}

fn closed_mouse(mouse: MouseEvent, state: &mut ClientState, position: Position) -> Handled {
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return Handled::Passed;
    }
    if state.chrome.handle_area.contains(position) {
        open(state, Panel::People);
        return Handled::Consumed;
    }
    if state.chrome.chip_area.contains(position) {
        open(state, Panel::Session);
        return Handled::Consumed;
    }
    Handled::Passed
}

fn people_click(
    mouse: MouseEvent,
    state: &mut ClientState,
    position: Position,
    last: &mut Option<(String, usize, Instant)>,
) -> io::Result<Handled> {
    let Some((index, row)) = state
        .people_areas
        .iter()
        .find(|(_, area)| area.contains(position))
        .copied()
    else {
        return Ok(Handled::Consumed);
    };
    if mouse.kind == MouseEventKind::Down(MouseButton::Right) {
        crate::person_menu::open(state, index, row);
        return Ok(Handled::Consumed);
    }
    state.select_person(index);
    if crate::input::double_click(last, format!("person:{}", state.user()), index) {
        state.open_focused();
        close(state);
    }
    Ok(Handled::Consumed)
}

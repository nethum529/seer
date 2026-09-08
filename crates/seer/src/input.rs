mod selection;
use crossterm::event::{KeyCode as CrosstermKeyCode, KeyEvent, KeyModifiers};
use seer_core::{InputEvent, KeyCode, KeyInput, Modifiers, TerminalInput};
pub(crate) use selection::{Selection, copy_text};

const SCROLLBACK_PAGE_LINES: i32 = 20;

pub(crate) fn key_to_input(key: KeyEvent) -> Option<TerminalInput> {
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        match key.code {
            CrosstermKeyCode::PageUp => {
                return Some(TerminalInput::new(InputEvent::Scrollback {
                    lines: SCROLLBACK_PAGE_LINES,
                }));
            }
            CrosstermKeyCode::PageDown => {
                return Some(TerminalInput::new(InputEvent::Scrollback {
                    lines: -SCROLLBACK_PAGE_LINES,
                }));
            }
            _ => {}
        }
    }
    let code = map_key_code(key.code)?;
    Some(TerminalInput::new(InputEvent::Key(KeyInput {
        code,
        modifiers: map_modifiers(key.modifiers),
    })))
}

fn map_key_code(code: CrosstermKeyCode) -> Option<KeyCode> {
    Some(match code {
        CrosstermKeyCode::Backspace => KeyCode::Backspace,
        CrosstermKeyCode::Enter => KeyCode::Enter,
        CrosstermKeyCode::Left => KeyCode::Left,
        CrosstermKeyCode::Right => KeyCode::Right,
        CrosstermKeyCode::Up => KeyCode::Up,
        CrosstermKeyCode::Down => KeyCode::Down,
        CrosstermKeyCode::Home => KeyCode::Home,
        CrosstermKeyCode::End => KeyCode::End,
        CrosstermKeyCode::PageUp => KeyCode::PageUp,
        CrosstermKeyCode::PageDown => KeyCode::PageDown,
        CrosstermKeyCode::Tab => KeyCode::Tab,
        CrosstermKeyCode::BackTab => KeyCode::BackTab,
        CrosstermKeyCode::Delete => KeyCode::Delete,
        CrosstermKeyCode::Insert => KeyCode::Insert,
        CrosstermKeyCode::F(number) => KeyCode::Function(number),
        CrosstermKeyCode::Char(character) => KeyCode::Char(character),
        code => return map_extended_key_code(code),
    })
}

fn map_extended_key_code(code: CrosstermKeyCode) -> Option<KeyCode> {
    Some(match code {
        CrosstermKeyCode::Null => KeyCode::Char('\0'),
        CrosstermKeyCode::Esc => KeyCode::Escape,
        CrosstermKeyCode::KeypadBegin => KeyCode::Begin,
        CrosstermKeyCode::CapsLock
        | CrosstermKeyCode::ScrollLock
        | CrosstermKeyCode::NumLock
        | CrosstermKeyCode::PrintScreen
        | CrosstermKeyCode::Pause
        | CrosstermKeyCode::Menu
        | CrosstermKeyCode::Media(_)
        | CrosstermKeyCode::Modifier(_) => return None,
        CrosstermKeyCode::Backspace
        | CrosstermKeyCode::Enter
        | CrosstermKeyCode::Left
        | CrosstermKeyCode::Right
        | CrosstermKeyCode::Up
        | CrosstermKeyCode::Down
        | CrosstermKeyCode::Home
        | CrosstermKeyCode::End
        | CrosstermKeyCode::PageUp
        | CrosstermKeyCode::PageDown
        | CrosstermKeyCode::Tab
        | CrosstermKeyCode::BackTab
        | CrosstermKeyCode::Delete
        | CrosstermKeyCode::Insert
        | CrosstermKeyCode::F(_)
        | CrosstermKeyCode::Char(_) => return None,
    })
}

pub(crate) fn map_modifiers(modifiers: KeyModifiers) -> Modifiers {
    Modifiers {
        shift: modifiers.contains(KeyModifiers::SHIFT),
        alt: modifiers.contains(KeyModifiers::ALT),
        control: modifiers.contains(KeyModifiers::CONTROL),
        super_key: modifiers.contains(KeyModifiers::SUPER),
        hyper: modifiers.contains(KeyModifiers::HYPER),
        meta: modifiers.contains(KeyModifiers::META),
    }
}

pub(crate) fn raw_bytes(input: &TerminalInput) -> std::io::Result<Option<Vec<u8>>> {
    seer_runtime::PaneGrid::new(1, 1).handle_input(input)
}

use crate::{state::ClientState, tui_navigation as navigation};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;
use seer_net::Stream;
use std::{
    io,
    time::{Duration, Instant},
};

pub(crate) fn command(
    key: KeyEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<bool> {
    if state.chrome.context.is_some() {
        crate::person_menu::context_key(key, stream, state)?;
        return Ok(false);
    }
    if state.quit_prompt {
        return navigation::key(key, stream, state);
    }
    if state.menu.is_some() {
        crate::person_menu::key(key, stream, state)?;
        return Ok(false);
    }
    if key.code == CrosstermKeyCode::Char('p')
        && key.modifiers.is_empty()
        && state.chrome.narrow
        && state.viewer.is_none()
        && !state.searching
    {
        state.chrome.show_people = !state.chrome.show_people;
        return Ok(false);
    }
    if key.code == CrosstermKeyCode::Char('m')
        && key.modifiers.is_empty()
        && state.viewer.is_none()
        && !state.searching
    {
        let row = state
            .people_areas
            .iter()
            .find(|(i, _)| *i == state.selected)
            .map_or(state.chrome.people_area, |(_, row)| *row);
        crate::person_menu::open(state, state.selected, row);
    } else if state.viewer.is_some() {
        crate::viewer::key(key, stream, state)?;
    } else {
        return navigation::key(key, stream, state);
    }
    Ok(false)
}

pub(crate) fn mouse(
    mouse: MouseEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
    last: &mut Option<(String, usize, Instant)>,
) -> io::Result<bool> {
    if mouse.modifiers.contains(KeyModifiers::SHIFT) {
        return Ok(false);
    }
    if selection::mouse(mouse, state)? {
        *last = None;
        return Ok(false);
    }
    let position = Position::new(mouse.column, mouse.row);
    if state.quit_prompt {
        return click_hint(mouse, stream, state, true);
    }
    if state.chrome.context.is_some() {
        crate::person_menu::context_mouse(mouse, stream, state)?;
        return Ok(false);
    }
    if state.menu.is_some() {
        crate::person_menu::mouse(mouse, stream, state)?;
        return Ok(false);
    }
    if matches!(
        mouse.kind,
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
    ) {
        scroll(mouse, state);
        return Ok(false);
    }
    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left | MouseButton::Right)
    ) {
        return Ok(false);
    }
    if state
        .chrome
        .footer_areas
        .iter()
        .any(|(_, area)| area.contains(position))
    {
        return click_hint(mouse, stream, state, false);
    }
    click_target(mouse, stream, state, last)?;
    Ok(false)
}

fn click_hint(
    mouse: MouseEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
    dialog: bool,
) -> io::Result<bool> {
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return Ok(false);
    }
    let areas = if dialog {
        &state.chrome.dialog_areas
    } else {
        &state.chrome.footer_areas
    };
    let key = areas
        .iter()
        .find(|(_, area)| area.contains(Position::new(mouse.column, mouse.row)))
        .map(|(key, _)| key.clone());
    let Some(key) = key else {
        return Ok(false);
    };
    let (code, modifiers) = match key.as_str() {
        "enter" => (CrosstermKeyCode::Enter, KeyModifiers::NONE),
        "esc" => (CrosstermKeyCode::Esc, KeyModifiers::NONE),
        "ctrl+b" => (CrosstermKeyCode::Char('b'), KeyModifiers::CONTROL),
        _ => match key.chars().next() {
            Some(character) => (CrosstermKeyCode::Char(character), KeyModifiers::NONE),
            None => return Ok(false),
        },
    };
    command(KeyEvent::new(code, modifiers), stream, state)
}

fn click_target(
    mouse: MouseEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
    last: &mut Option<(String, usize, Instant)>,
) -> io::Result<()> {
    let position = Position::new(mouse.column, mouse.row);
    let right = mouse.kind == MouseEventKind::Down(MouseButton::Right);
    if let Some((index, row)) = state
        .people_areas
        .iter()
        .find(|(_, area)| area.contains(position))
        .copied()
    {
        if right {
            crate::person_menu::open(state, index, row);
        } else {
            state.select_person(index);
            if double_click(last, format!("person:{}", state.user()), index) {
                state.open_focused();
            }
        }
        return Ok(());
    }
    if state.plus_area.contains(position) && !right {
        navigation::key(
            KeyEvent::new(CrosstermKeyCode::Char('n'), KeyModifiers::NONE),
            stream,
            state,
        )?;
        return Ok(());
    }
    if let Some((index, area)) = state
        .tab_areas
        .iter()
        .find(|(_, area)| area.contains(position))
        .copied()
    {
        state.select_tab(index);
        if right {
            crate::person_menu::open_context(state, position);
        } else if state.user() == state.own_user && mouse.column == area.right().saturating_sub(2) {
            state.request_close(stream)?;
        }
        return Ok(());
    }
    if let Some(index) = state
        .box_areas
        .iter()
        .find(|tile| tile.area.contains(position))
        .map(|tile| tile.index)
    {
        state.select_tab(index);
        if right {
            crate::person_menu::open_context(state, position);
        } else if double_click(last, format!("box:{}", state.user()), index) {
            state.open_focused();
        }
    }
    Ok(())
}

fn double_click(last: &mut Option<(String, usize, Instant)>, target: String, index: usize) -> bool {
    if last.as_ref().is_some_and(|(old, i, time)| {
        *old == target && *i == index && time.elapsed() < Duration::from_millis(400)
    }) {
        *last = None;
        true
    } else {
        *last = Some((target, index, Instant::now()));
        false
    }
}

fn scroll(mouse: MouseEvent, state: &mut ClientState) {
    let position = Position::new(mouse.column, mouse.row);
    let up = mouse.kind == MouseEventKind::ScrollUp;
    if state.chrome.people_area.contains(position) {
        state.people_scroll = if up {
            state.people_scroll.saturating_sub(1)
        } else {
            state.people_scroll.saturating_add(1)
        };
    } else if let Some(viewer) = &mut state.viewer {
        if viewer.area.contains(position) {
            viewer.scroll(up);
        }
    } else {
        state.grid_scroll = if up {
            state.grid_scroll.saturating_sub(1)
        } else {
            state.grid_scroll.saturating_add(1)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use seer_core::Tree;
    use std::os::unix::net::UnixStream;

    #[test]
    fn people_toggle_only_changes_narrow_screens() {
        let mut state = ClientState::new(Tree::new(), "alice".into());
        let (mut stream, _peer) = UnixStream::pair().expect("streams must open");
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend must open");
        terminal
            .draw(|frame| crate::render::draw(frame, &mut state))
            .expect("screen must draw");
        let key = KeyEvent::new(CrosstermKeyCode::Char('p'), KeyModifiers::NONE);
        command(key, &mut stream, &mut state).expect("wide key must work");
        terminal.backend_mut().resize(45, 30);
        terminal
            .draw(|frame| crate::render::draw(frame, &mut state))
            .expect("narrow screen must draw");
        assert!(!title_row(&terminal).contains("people"));
        command(key, &mut stream, &mut state).expect("narrow key must work");
        terminal
            .draw(|frame| crate::render::draw(frame, &mut state))
            .expect("people must draw");
        assert!(title_row(&terminal).contains("people"));
        command(key, &mut stream, &mut state).expect("narrow key must work");
        terminal
            .draw(|frame| crate::render::draw(frame, &mut state))
            .expect("people must hide");
        assert!(!title_row(&terminal).contains("people"));
    }

    fn title_row(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        (0..buffer.area.width)
            .map(|x| buffer[(x, 1)].symbol())
            .collect()
    }
}

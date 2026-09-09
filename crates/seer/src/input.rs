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

use crate::state::ClientState;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;
use std::io;

pub(crate) fn command(
    key: KeyEvent,
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<bool> {
    if state.chrome.context.is_some() {
        crate::person_menu::context_key(key, stream, state)?;
        return Ok(false);
    }
    if state.menu.is_some() {
        crate::person_menu::key(key, stream, state)?;
        return Ok(false);
    }
    if state.chrome.panel.is_some() {
        return crate::panels::key(key, stream, state);
    }
    crate::viewer::key(key, stream, state)?;
    Ok(false)
}

pub(crate) fn mouse(
    mouse: MouseEvent,
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<bool> {
    if mouse.modifiers.contains(KeyModifiers::SHIFT) {
        return Ok(false);
    }
    // A live selection means the press landed on terminal content, so chrome
    // that consumed the press cannot release into the terminal behind it.
    let pressed_content = state.selection.is_some();
    if state.chrome.context.is_some() {
        crate::person_menu::context_mouse(mouse, stream, state)?;
        return Ok(false);
    }
    if state.menu.is_some() {
        crate::person_menu::mouse(mouse, stream, state)?;
        return Ok(false);
    }
    match crate::panels::mouse(mouse, stream, state)? {
        crate::panels::Handled::Consumed => return Ok(false),
        crate::panels::Handled::Quit => return Ok(true),
        crate::panels::Handled::Passed => {}
    }
    if selection::mouse(mouse, state)? {
        return Ok(false);
    }
    if matches!(
        mouse.kind,
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
    ) {
        scroll(mouse, state);
        return Ok(false);
    }
    if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
        if pressed_content {
            open_clicked(mouse, state);
        }
        return Ok(false);
    }
    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left | MouseButton::Right)
    ) {
        return Ok(false);
    }
    click_target(mouse, state);
    Ok(false)
}

fn click_target(mouse: MouseEvent, state: &mut ClientState) {
    let position = Position::new(mouse.column, mouse.row);
    if let Some(index) = state
        .box_areas
        .iter()
        .find(|tile| tile.area.contains(position))
        .map(|tile| tile.index)
    {
        state.select_tab(index);
        if mouse.kind == MouseEventKind::Down(MouseButton::Right) {
            crate::person_menu::open_context(state, position);
        }
    }
}

fn open_clicked(mouse: MouseEvent, state: &mut ClientState) {
    let position = Position::new(mouse.column, mouse.row);
    if let Some(index) = state
        .box_areas
        .iter()
        .find(|tile| tile.content.contains(position))
        .map(|tile| tile.index)
    {
        state.select_tab(index);
        state.open_focused();
    }
}

fn scroll(mouse: MouseEvent, state: &mut ClientState) {
    let position = Position::new(mouse.column, mouse.row);
    let up = mouse.kind == MouseEventKind::ScrollUp;
    if let Some(viewer) = &mut state.viewer {
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
mod keyboard_tests;
#[cfg(test)]
mod mouse_tests;
#[cfg(test)]
mod tests;

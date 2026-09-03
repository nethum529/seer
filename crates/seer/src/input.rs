use crossterm::event::{
    KeyCode as CrosstermKeyCode, KeyEvent, KeyModifiers, MouseButton as CrosstermMouseButton,
    MouseEvent, MouseEventKind,
};
use seer_core::{
    InputEvent, KeyCode, KeyInput, Modifiers, MouseButton, MouseInput, MouseKind, TerminalInput,
};

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

pub(crate) fn mouse_to_input(mouse: MouseEvent, column: u16, row: u16) -> TerminalInput {
    let (kind, button) = map_mouse_kind(mouse.kind);
    TerminalInput::new(InputEvent::Mouse(MouseInput {
        kind,
        button,
        column,
        row,
        modifiers: map_modifiers(mouse.modifiers),
    }))
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

fn map_mouse_kind(kind: MouseEventKind) -> (MouseKind, Option<MouseButton>) {
    match kind {
        MouseEventKind::Down(button) => (MouseKind::Down, Some(map_mouse_button(button))),
        MouseEventKind::Up(button) => (MouseKind::Up, Some(map_mouse_button(button))),
        MouseEventKind::Drag(button) => (MouseKind::Drag, Some(map_mouse_button(button))),
        MouseEventKind::Moved => (MouseKind::Moved, None),
        MouseEventKind::ScrollDown => (MouseKind::ScrollDown, None),
        MouseEventKind::ScrollUp => (MouseKind::ScrollUp, None),
        MouseEventKind::ScrollLeft => (MouseKind::ScrollLeft, None),
        MouseEventKind::ScrollRight => (MouseKind::ScrollRight, None),
    }
}

fn map_mouse_button(button: CrosstermMouseButton) -> MouseButton {
    match button {
        CrosstermMouseButton::Left => MouseButton::Left,
        CrosstermMouseButton::Middle => MouseButton::Middle,
        CrosstermMouseButton::Right => MouseButton::Right,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use seer_core::{InputEvent, KeyCode as CoreKeyCode};

    use super::key_to_input;

    #[test]
    fn encodes_supported_keys() {
        let cases = [
            (key(KeyCode::Char('a')), CoreKeyCode::Char('a')),
            (key(KeyCode::Enter), CoreKeyCode::Enter),
            (key(KeyCode::Backspace), CoreKeyCode::Backspace),
            (key(KeyCode::Up), CoreKeyCode::Up),
            (key(KeyCode::Delete), CoreKeyCode::Delete),
            (key(KeyCode::F(12)), CoreKeyCode::Function(12)),
        ];

        for (key, expected) in cases {
            let input = key_to_input(key).expect("key must map");
            let InputEvent::Key(key) = input.event else {
                panic!("key event expected");
            };
            assert_eq!(key.code, expected);
        }
    }

    #[test]
    fn ignores_non_terminal_keys() {
        assert_eq!(key_to_input(key(KeyCode::CapsLock)), None);
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
}

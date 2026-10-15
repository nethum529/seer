use gpui::Keystroke;
use seer_core::{InputEvent, KeyCode, KeyInput, Modifiers};

pub(crate) fn modifiers(held: gpui::Modifiers) -> Modifiers {
    Modifiers {
        shift: held.shift,
        alt: held.alt,
        control: held.control,
        super_key: held.platform,
        ..Modifiers::default()
    }
}

pub(crate) fn event(key: &Keystroke) -> Option<InputEvent> {
    let held = key.modifiers;
    if held.platform {
        return None;
    }
    if let Some(code) = text_key(&key.key) {
        return Some(native_key(key, code));
    }
    if key.key == "keypadenter" {
        let text = keypad_enter(key)?;
        let prefix = if held.alt && !cfg!(target_os = "macos") {
            "\x1b"
        } else {
            ""
        };
        return Some(InputEvent::Text(format!("{prefix}{text}")));
    }
    if let Some(code) = named_key(&key.key) {
        return Some(InputEvent::Key(KeyInput {
            code,
            modifiers: modifiers(held),
        }));
    }
    if held.control && (key.key == "space" || key.key.chars().count() == 1) {
        let character = if key.key == "space" {
            ' '
        } else {
            key.key.chars().next()?
        };
        return Some(native_key(key, KeyCode::Char(character)));
    }
    if held.control && cfg!(target_os = "macos") {
        return key.key_char.clone().map(InputEvent::Text);
    }
    if held.alt && !cfg!(target_os = "macos") && key.key_char.is_some() {
        let text = key.key_char.as_deref().unwrap_or(&key.key);
        if key.key.chars().count() == 1 {
            return Some(native_key(key, KeyCode::Char(key.key.chars().next()?)));
        }
        return Some(InputEvent::Text(format!("\x1b{text}")));
    }
    None
}

fn native_key(key: &Keystroke, code: KeyCode) -> InputEvent {
    InputEvent::KeyWithText {
        key: KeyInput {
            code,
            modifiers: modifiers(key.modifiers),
        },
        text: key.key_char.clone(),
    }
}

fn text_key(key: &str) -> Option<KeyCode> {
    Some(match key {
        "enter" => KeyCode::Enter,
        "backspace" => KeyCode::Backspace,
        "tab" => KeyCode::Tab,
        "escape" => KeyCode::Escape,
        _ => return None,
    })
}

fn keypad_enter(key: &Keystroke) -> Option<&str> {
    let held = key.modifiers;
    if cfg!(target_os = "macos") && (held.control || held.alt || held.shift) {
        key.key_char.as_deref()
    } else {
        Some("\n")
    }
}

fn named_key(key: &str) -> Option<KeyCode> {
    Some(match key {
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "insert" => KeyCode::Insert,
        "delete" => KeyCode::Delete,
        _ => KeyCode::Function(key.strip_prefix('f')?.parse().ok()?),
    })
}

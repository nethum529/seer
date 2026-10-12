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
    if let Some(text) = control_key(key) {
        return Some(InputEvent::Text(text));
    }
    if let Some(code) = named_key(&key.key) {
        return Some(InputEvent::Key(KeyInput {
            code,
            modifiers: modifiers(held),
        }));
    }
    if held.control && cfg!(target_os = "macos") {
        return key.key_char.clone().map(InputEvent::Text);
    }
    if held.control && (key.key == "space" || key.key.chars().count() == 1) {
        let character = if key.key == "space" {
            ' '
        } else {
            key.key.chars().next()?
        };
        return Some(InputEvent::Key(KeyInput {
            code: KeyCode::Char(character),
            modifiers: modifiers(held),
        }));
    }
    if held.alt && !cfg!(target_os = "macos") && key.key_char.is_some() {
        let text = key.key_char.as_deref().unwrap_or(&key.key);
        return Some(InputEvent::Text(format!("\x1b{text}")));
    }
    None
}

fn control_key(key: &Keystroke) -> Option<String> {
    let held = key.modifiers;
    let text = match key.key.as_str() {
        "enter" => "\r",
        "keypadenter" => keypad_enter(key)?,
        "backspace" if held.control && !cfg!(target_os = "macos") => "\x08",
        "backspace" => "\x7f",
        "tab" if held.shift && held.control && cfg!(target_os = "macos") => "\x19",
        "tab" if held.shift && !held.control => "\x1b[Z",
        "tab" => "\t",
        "escape" => "\x1b",
        _ => return None,
    };
    let prefix = if alt_prefix(key) { "\x1b" } else { "" };
    Some(format!("{prefix}{text}"))
}

fn keypad_enter(key: &Keystroke) -> Option<&str> {
    let held = key.modifiers;
    if cfg!(target_os = "macos") && (held.control || held.alt || held.shift) {
        key.key_char.as_deref()
    } else {
        Some("\n")
    }
}

fn alt_prefix(key: &Keystroke) -> bool {
    let held = key.modifiers;
    let binding = match key.key.as_str() {
        "backspace" => !held.shift && !held.control,
        "tab" => held.shift && !held.control,
        _ => false,
    };
    held.alt && (!cfg!(target_os = "macos") || binding)
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

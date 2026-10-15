use alacritty_terminal::term::TermMode;
use seer_core::{KeyCode, KeyInput};

use super::encode_key;

pub(crate) fn encode_native_key(key: KeyInput, text: Option<&str>, mode: TermMode) -> Vec<u8> {
    let held = key.modifiers;
    if cfg!(target_os = "macos")
        && key.code == KeyCode::Backspace
        && held.alt
        && !held.control
        && !held.shift
    {
        return b"\x1b\x7f".to_vec();
    }
    if mode.contains(TermMode::DISAMBIGUATE_ESC_CODES) {
        let mut key = key;
        if cfg!(target_os = "macos") {
            key.modifiers.alt = false;
        }
        return encode_key(key, mode);
    }
    if let Some(control) = control_text(key) {
        let prefix = if alt_prefix(key) { "\x1b" } else { "" };
        return format!("{prefix}{control}").into_bytes();
    }
    if cfg!(target_os = "macos") && held.control {
        return text.unwrap_or_default().as_bytes().to_vec();
    }
    if !held.control
        && let Some(text) = text
    {
        let prefix = if held.alt && !cfg!(target_os = "macos") {
            "\x1b"
        } else {
            ""
        };
        return format!("{prefix}{text}").into_bytes();
    }
    encode_key(key, mode)
}

fn control_text(key: KeyInput) -> Option<&'static str> {
    let held = key.modifiers;
    Some(match key.code {
        KeyCode::Enter => "\r",
        KeyCode::Backspace if held.control && !cfg!(target_os = "macos") => "\x08",
        KeyCode::Backspace => "\x7f",
        KeyCode::Tab if held.shift && held.control && cfg!(target_os = "macos") => "\x19",
        KeyCode::Tab if held.shift && !held.control => "\x1b[Z",
        KeyCode::Tab => "\t",
        KeyCode::Escape => "\x1b",
        _ => return None,
    })
}

fn alt_prefix(key: KeyInput) -> bool {
    let held = key.modifiers;
    let binding = match key.code {
        KeyCode::Backspace => !held.shift && !held.control,
        KeyCode::Tab => held.shift && !held.control,
        _ => false,
    };
    held.alt && (!cfg!(target_os = "macos") || binding)
}

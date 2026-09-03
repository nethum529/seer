use alacritty_terminal::term::TermMode;
use seer_core::{KeyCode, KeyInput, Modifiers, MouseButton, MouseInput, MouseKind};

pub(crate) fn encode_key(key: KeyInput, mode: TermMode) -> Vec<u8> {
    let bytes = match key.code {
        KeyCode::Char(character) => encode_character(character, key.modifiers),
        KeyCode::Backspace => modified_control(127, key.modifiers),
        KeyCode::Enter => modified_control(13, key.modifiers),
        KeyCode::Tab => modified_control(9, key.modifiers),
        KeyCode::Escape => modified_control(27, key.modifiers),
        KeyCode::BackTab => back_tab(key.modifiers),
        KeyCode::Up => modified_csi('A', key.modifiers, mode.contains(TermMode::APP_CURSOR)),
        KeyCode::Down => modified_csi('B', key.modifiers, mode.contains(TermMode::APP_CURSOR)),
        KeyCode::Right => modified_csi('C', key.modifiers, mode.contains(TermMode::APP_CURSOR)),
        KeyCode::Left => modified_csi('D', key.modifiers, mode.contains(TermMode::APP_CURSOR)),
        KeyCode::Home => modified_csi('H', key.modifiers, mode.contains(TermMode::APP_CURSOR)),
        KeyCode::End => modified_csi('F', key.modifiers, mode.contains(TermMode::APP_CURSOR)),
        KeyCode::Begin => modified_csi('E', key.modifiers, false),
        KeyCode::Insert => tilde_key(2, key.modifiers),
        KeyCode::Delete => tilde_key(3, key.modifiers),
        KeyCode::PageUp => tilde_key(5, key.modifiers),
        KeyCode::PageDown => tilde_key(6, key.modifiers),
        KeyCode::Function(number) => function_key(number, key.modifiers),
    };
    add_alt_prefix(bytes, key.modifiers)
}

fn encode_character(character: char, modifiers: Modifiers) -> Vec<u8> {
    if modifiers.super_key || modifiers.hyper || modifiers.meta {
        return format!(
            "\x1b[{};{}u",
            character as u32,
            modifier_parameter(modifiers)
        )
        .into_bytes();
    }
    if modifiers.control
        && let Some(control) = control_character(character)
    {
        return vec![control];
    }
    let mut encoded = [0; 4];
    character.encode_utf8(&mut encoded).as_bytes().to_vec()
}

fn control_character(character: char) -> Option<u8> {
    match character {
        ' ' | '@' | '`' | '2' => Some(0),
        'a'..='z' => Some(character as u8 - b'a' + 1),
        'A'..='Z' => Some(character as u8 - b'A' + 1),
        '[' | '3' => Some(27),
        '\\' | '4' => Some(28),
        ']' | '5' => Some(29),
        '^' | '6' => Some(30),
        '_' | '7' | '/' => Some(31),
        '?' | '8' => Some(127),
        _ => None,
    }
}

fn modified_control(codepoint: u8, modifiers: Modifiers) -> Vec<u8> {
    if has_non_alt_modifier(modifiers) {
        format!("\x1b[{codepoint};{}u", modifier_parameter(modifiers)).into_bytes()
    } else {
        vec![codepoint]
    }
}

fn back_tab(modifiers: Modifiers) -> Vec<u8> {
    let only_shift = modifiers.shift
        && !modifiers.alt
        && !modifiers.control
        && !modifiers.super_key
        && !modifiers.hyper
        && !modifiers.meta;
    if only_shift {
        b"\x1b[Z".to_vec()
    } else {
        modified_csi('Z', modifiers, false)
    }
}

fn modified_csi(final_byte: char, modifiers: Modifiers, application: bool) -> Vec<u8> {
    if has_modifiers(modifiers) {
        format!("\x1b[1;{}{final_byte}", modifier_parameter(modifiers)).into_bytes()
    } else if application {
        format!("\x1bO{final_byte}").into_bytes()
    } else {
        format!("\x1b[{final_byte}").into_bytes()
    }
}

fn tilde_key(code: u8, modifiers: Modifiers) -> Vec<u8> {
    if has_modifiers(modifiers) {
        format!("\x1b[{code};{}~", modifier_parameter(modifiers)).into_bytes()
    } else {
        format!("\x1b[{code}~").into_bytes()
    }
}

fn function_key(number: u8, modifiers: Modifiers) -> Vec<u8> {
    match number {
        1..=4 => function_key_ss3(number, modifiers),
        _ => function_key_tilde(number, modifiers),
    }
}

fn function_key_ss3(number: u8, modifiers: Modifiers) -> Vec<u8> {
    let final_byte = char::from(b'P' + number - 1);
    if has_modifiers(modifiers) {
        format!("\x1b[1;{}{final_byte}", modifier_parameter(modifiers)).into_bytes()
    } else {
        format!("\x1bO{final_byte}").into_bytes()
    }
}

fn function_key_tilde(number: u8, modifiers: Modifiers) -> Vec<u8> {
    let code = match number {
        5 => 15,
        6 => 17,
        7 => 18,
        8 => 19,
        9 => 20,
        10 => 21,
        11 => 23,
        12 => 24,
        13 => 25,
        14 => 26,
        15 => 28,
        16 => 29,
        17 => 31,
        18 => 32,
        19 => 33,
        20 => 34,
        _ => return Vec::new(),
    };
    tilde_key(code, modifiers)
}

fn add_alt_prefix(mut bytes: Vec<u8>, modifiers: Modifiers) -> Vec<u8> {
    if modifiers.alt && !bytes.starts_with(b"\x1b[") && !bytes.starts_with(b"\x1bO") {
        bytes.insert(0, 0x1b);
    }
    bytes
}

fn has_non_alt_modifier(modifiers: Modifiers) -> bool {
    modifiers.shift || modifiers.control || modifiers.super_key || modifiers.hyper || modifiers.meta
}

fn has_modifiers(modifiers: Modifiers) -> bool {
    has_non_alt_modifier(modifiers) || modifiers.alt
}

fn modifier_parameter(modifiers: Modifiers) -> u8 {
    1 + u8::from(modifiers.shift)
        + 2 * u8::from(modifiers.alt)
        + 4 * u8::from(modifiers.control)
        + 8 * u8::from(modifiers.super_key || modifiers.hyper || modifiers.meta)
}

pub(crate) fn encode_mouse(mouse: MouseInput, mode: TermMode) -> Option<Vec<u8>> {
    if !reports_mouse(mouse.kind, mode) {
        return None;
    }
    let code = if mouse.kind == MouseKind::Up && !mode.contains(TermMode::SGR_MOUSE) {
        3
    } else {
        mouse_code(mouse)
    } + mouse_modifier_code(mouse.modifiers);
    if mode.contains(TermMode::SGR_MOUSE) {
        let suffix = if mouse.kind == MouseKind::Up {
            'm'
        } else {
            'M'
        };
        Some(
            format!(
                "\x1b[<{code};{};{}{suffix}",
                mouse.column.saturating_add(1),
                mouse.row.saturating_add(1)
            )
            .into_bytes(),
        )
    } else {
        encode_legacy_mouse(code, mouse.column, mouse.row, mode)
    }
}

fn reports_mouse(kind: MouseKind, mode: TermMode) -> bool {
    match kind {
        MouseKind::Moved => mode.contains(TermMode::MOUSE_MOTION),
        MouseKind::Drag => mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION),
        _ => mode.intersects(TermMode::MOUSE_MODE),
    }
}

fn mouse_code(mouse: MouseInput) -> u8 {
    let button = match mouse.button {
        Some(MouseButton::Left) => 0,
        Some(MouseButton::Middle) => 1,
        Some(MouseButton::Right) => 2,
        None => 3,
    };
    match mouse.kind {
        MouseKind::Down => button,
        MouseKind::Up => button,
        MouseKind::Drag => button + 32,
        MouseKind::Moved => 35,
        MouseKind::ScrollUp => 64,
        MouseKind::ScrollDown => 65,
        MouseKind::ScrollLeft => 66,
        MouseKind::ScrollRight => 67,
    }
}

fn mouse_modifier_code(modifiers: Modifiers) -> u8 {
    4 * u8::from(modifiers.shift) + 8 * u8::from(modifiers.alt) + 16 * u8::from(modifiers.control)
}

fn encode_legacy_mouse(code: u8, column: u16, row: u16, mode: TermMode) -> Option<Vec<u8>> {
    let codepoint = u32::from(code) + 32;
    let column = u32::from(column) + 33;
    let row = u32::from(row) + 33;
    if mode.contains(TermMode::UTF8_MOUSE) {
        let payload: String = [codepoint, column, row]
            .into_iter()
            .map(char::from_u32)
            .collect::<Option<_>>()?;
        Some([b"\x1b[M".as_slice(), payload.as_bytes()].concat())
    } else {
        Some(vec![
            0x1b,
            b'[',
            b'M',
            u8::try_from(codepoint).ok()?,
            u8::try_from(column).ok()?,
            u8::try_from(row).ok()?,
        ])
    }
}

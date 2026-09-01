use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn key_to_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    let command_modifiers = KeyModifiers::CONTROL
        | KeyModifiers::ALT
        | KeyModifiers::SUPER
        | KeyModifiers::HYPER
        | KeyModifiers::META;
    if key.modifiers.intersects(command_modifiers) {
        return None;
    }

    match key.code {
        KeyCode::Char(character) => {
            let mut encoded = [0; 4];
            Some(character.encode_utf8(&mut encoded).as_bytes().to_vec())
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::key_to_bytes;

    #[test]
    fn encodes_supported_keys() {
        let cases = [
            (key(KeyCode::Char('a')), b"a".as_slice()),
            (
                KeyEvent::new(KeyCode::Char('Z'), KeyModifiers::SHIFT),
                b"Z".as_slice(),
            ),
            (key(KeyCode::Char('\u{00e9}')), "\u{00e9}".as_bytes()),
            (key(KeyCode::Enter), b"\r".as_slice()),
            (key(KeyCode::Backspace), b"\x7f".as_slice()),
            (key(KeyCode::Up), b"\x1b[A".as_slice()),
            (key(KeyCode::Down), b"\x1b[B".as_slice()),
            (key(KeyCode::Right), b"\x1b[C".as_slice()),
            (key(KeyCode::Left), b"\x1b[D".as_slice()),
        ];

        for (key, expected) in cases {
            assert_eq!(key_to_bytes(key).as_deref(), Some(expected));
        }
    }

    #[test]
    fn ignores_unsupported_keys() {
        assert_eq!(key_to_bytes(key(KeyCode::Esc)), None);
        assert_eq!(key_to_bytes(key(KeyCode::Tab)), None);
        assert_eq!(key_to_bytes(key(KeyCode::F(1))), None);
        assert_eq!(
            key_to_bytes(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            None
        );
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
}

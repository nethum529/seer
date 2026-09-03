use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use seer_core::SplitDirection;
use seer_core::Tree;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FocusDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum InputAction {
    CreateTab,
    SplitPane(SplitDirection),
    ClosePane,
    NextTab,
    PreviousTab,
    FocusPane(FocusDirection),
    FocusNumber(usize),
    Bytes(Vec<u8>),
}

pub(crate) fn key_to_action(key: KeyEvent, prefix_pending: &mut bool) -> Option<InputAction> {
    if !*prefix_pending {
        if is_control_char(key, 'b') {
            *prefix_pending = true;
            return None;
        }
        return key_to_bytes(key).map(InputAction::Bytes);
    }

    *prefix_pending = false;
    if is_control_char(key, 'b') {
        return Some(InputAction::Bytes(vec![0x02]));
    }
    let command_modifiers = KeyModifiers::CONTROL
        | KeyModifiers::ALT
        | KeyModifiers::SUPER
        | KeyModifiers::HYPER
        | KeyModifiers::META;
    if key.modifiers.intersects(command_modifiers) {
        return None;
    }
    match key.code {
        KeyCode::Char('c') => Some(InputAction::CreateTab),
        KeyCode::Char('%') => Some(InputAction::SplitPane(SplitDirection::Right)),
        KeyCode::Char('"') => Some(InputAction::SplitPane(SplitDirection::Down)),
        KeyCode::Char('x') => Some(InputAction::ClosePane),
        KeyCode::Char('n') => Some(InputAction::NextTab),
        KeyCode::Char('p') => Some(InputAction::PreviousTab),
        KeyCode::Up => Some(InputAction::FocusPane(FocusDirection::Up)),
        KeyCode::Down => Some(InputAction::FocusPane(FocusDirection::Down)),
        KeyCode::Left => Some(InputAction::FocusPane(FocusDirection::Left)),
        KeyCode::Right => Some(InputAction::FocusPane(FocusDirection::Right)),
        KeyCode::Char(number @ '1'..='9') => number
            .to_digit(10)
            .map(|value| InputAction::FocusNumber(value as usize)),
        _ => None,
    }
}

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

pub(crate) fn selected_tab_tree(
    tree: &Tree,
    workspace_id: &str,
    current_tab: &str,
    forward: bool,
) -> Option<Tree> {
    let workspace = tree
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)?;
    let current = workspace
        .tabs
        .iter()
        .position(|tab| tab.id == current_tab)?;
    let next = if forward {
        (current + 1) % workspace.tabs.len()
    } else {
        (current + workspace.tabs.len() - 1) % workspace.tabs.len()
    };
    let selected_id = workspace.tabs.get(next)?.id.clone();
    let mut selected = tree.clone();
    selected
        .workspaces
        .retain(|workspace| workspace.id == workspace_id);
    let workspace = selected.workspaces.first_mut()?;
    workspace.tabs.retain(|tab| tab.id == selected_id);
    Some(selected)
}

pub(crate) fn pane_in_direction(
    rects: &[(String, Rect)],
    focused: &str,
    direction: FocusDirection,
) -> Option<String> {
    let source = rects.iter().find(|(pane, _)| pane == focused)?.1;
    rects
        .iter()
        .filter(|(pane, _)| pane != focused)
        .filter_map(|(pane, rect)| {
            direction_score(source, *rect, direction).map(|score| (score, pane))
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, pane)| pane.clone())
}

fn direction_score(source: Rect, target: Rect, direction: FocusDirection) -> Option<(u32, u32)> {
    let (left, top, right, bottom) = rect_edges(source);
    let (target_left, target_top, target_right, target_bottom) = rect_edges(target);
    let score = match direction {
        FocusDirection::Up if target_bottom <= top => (
            top - target_bottom,
            span_gap(left, right, target_left, target_right),
        ),
        FocusDirection::Down if target_top >= bottom => (
            target_top - bottom,
            span_gap(left, right, target_left, target_right),
        ),
        FocusDirection::Left if target_right <= left => (
            left - target_right,
            span_gap(top, bottom, target_top, target_bottom),
        ),
        FocusDirection::Right if target_left >= right => (
            target_left - right,
            span_gap(top, bottom, target_top, target_bottom),
        ),
        _ => return None,
    };
    Some(score)
}

fn span_gap(first_start: u32, first_end: u32, second_start: u32, second_end: u32) -> u32 {
    if first_end <= second_start {
        second_start - first_end
    } else {
        first_start.saturating_sub(second_end)
    }
}

fn rect_edges(rect: Rect) -> (u32, u32, u32, u32) {
    let left = u32::from(rect.x);
    let top = u32::from(rect.y);
    (
        left,
        top,
        left + u32::from(rect.width),
        top + u32::from(rect.height),
    )
}

fn is_control_char(key: KeyEvent, character: char) -> bool {
    key.code == KeyCode::Char(character) && key.modifiers == KeyModifiers::CONTROL
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{InputAction, key_to_action, key_to_bytes};

    #[test]
    fn prefix_creates_a_tab_and_sends_a_literal_prefix() {
        let mut prefix_pending = false;
        let prefix = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);

        assert_eq!(key_to_action(prefix, &mut prefix_pending), None);
        assert_eq!(
            key_to_action(key(KeyCode::Char('c')), &mut prefix_pending),
            Some(InputAction::CreateTab)
        );
        assert_eq!(key_to_action(prefix, &mut prefix_pending), None);
        assert_eq!(
            key_to_action(prefix, &mut prefix_pending),
            Some(InputAction::Bytes(vec![0x02]))
        );
    }

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

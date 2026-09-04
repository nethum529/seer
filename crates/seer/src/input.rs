use crossterm::event::{
    KeyCode as CrosstermKeyCode, KeyEvent, KeyModifiers, MouseButton as CrosstermMouseButton,
    MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use seer_core::{
    InputEvent, KeyCode, KeyInput, Modifiers, MouseButton, MouseInput, MouseKind, MouseTracking,
    SplitDirection, TerminalInput, Tree,
};

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
    ToggleDrawer,
    FocusPane(FocusDirection),
    FocusNumber(usize),
    Bytes(TerminalInput),
}

pub(crate) fn key_to_action(key: KeyEvent, prefix_pending: &mut bool) -> Option<InputAction> {
    if !*prefix_pending {
        if is_control_char(key, 'b') {
            *prefix_pending = true;
            return None;
        }
        return key_to_input(key).map(InputAction::Bytes);
    }

    *prefix_pending = false;
    if is_control_char(key, 'b') {
        return Some(InputAction::Bytes(TerminalInput::new(InputEvent::Text(
            "\u{2}".into(),
        ))));
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
        CrosstermKeyCode::Char('c') => Some(InputAction::CreateTab),
        CrosstermKeyCode::Char('%') => Some(InputAction::SplitPane(SplitDirection::Right)),
        CrosstermKeyCode::Char('"') => Some(InputAction::SplitPane(SplitDirection::Down)),
        CrosstermKeyCode::Char('x') => Some(InputAction::ClosePane),
        CrosstermKeyCode::Char('n') => Some(InputAction::NextTab),
        CrosstermKeyCode::Char('p') => Some(InputAction::PreviousTab),
        CrosstermKeyCode::Char('u') => Some(InputAction::ToggleDrawer),
        CrosstermKeyCode::Up => Some(InputAction::FocusPane(FocusDirection::Up)),
        CrosstermKeyCode::Down => Some(InputAction::FocusPane(FocusDirection::Down)),
        CrosstermKeyCode::Left => Some(InputAction::FocusPane(FocusDirection::Left)),
        CrosstermKeyCode::Right => Some(InputAction::FocusPane(FocusDirection::Right)),
        CrosstermKeyCode::Char(number @ '1'..='9') => number
            .to_digit(10)
            .map(|value| InputAction::FocusNumber(value as usize)),
        _ => None,
    }
}

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

pub(crate) fn mouse_event_is_tracked(kind: MouseEventKind, tracking: MouseTracking) -> bool {
    match kind {
        MouseEventKind::Moved => tracking == MouseTracking::AnyMotion,
        MouseEventKind::Drag(_) => {
            matches!(
                tracking,
                MouseTracking::ButtonMotion | MouseTracking::AnyMotion
            )
        }
        _ => true,
    }
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

pub(crate) fn is_control_char(key: KeyEvent, character: char) -> bool {
    key.code == CrosstermKeyCode::Char(character) && key.modifiers == KeyModifiers::CONTROL
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use seer_core::{InputEvent, KeyCode as CoreKeyCode, TerminalInput};

    use super::{InputAction, key_to_action, key_to_input};

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
            Some(InputAction::Bytes(TerminalInput::new(InputEvent::Text(
                "\u{2}".into()
            ))))
        );
    }

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
        assert_eq!(key_to_input(key(KeyCode::CapsLock)), None);
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
}

use super::*;

macro_rules! entry {
    ($action:expr, [$($scope:ident),+], $context:literal, $name:literal, $mac:expr, $linux:expr) => {
        Shortcut { action: $action, scopes: &[$(Scope::$scope),+], context: $context, name: $name, mac: $mac, linux: $linux }
    };
    ($name:literal, $digit:literal) => {
        entry!(Action::Terminal($digit), [Window], "Window", $name,
            &[chord(stringify!($digit), CMD, 0)], &[chord(stringify!($digit), CTRL | ALT | CMD, 0)])
    };
    ($action:expr, [$($scope:ident),+], $context:literal, $name:literal, $keys:expr) => {
        entry!($action, [$($scope),+], $context, $name, $keys, $keys)
    };
}

pub(crate) const SHORTCUTS: &[Shortcut] = &[
    entry!(
        Action::NewTerminal,
        [Window],
        "Window",
        "New terminal",
        &[chord("n", CMD, 0)],
        &[chord("n", CTRL | SHIFT, 0)]
    ),
    entry!(
        Action::ToggleSidebar,
        [Window],
        "Window",
        "Show or hide sidebar",
        &[chord("b", CMD, 0)],
        &[chord("b", CTRL | SHIFT, 0)]
    ),
    entry!(
        Action::FocusSidebar,
        [Window],
        "Window",
        "Focus sidebar",
        &[chord("b", CMD | SHIFT, 0)],
        &[chord("b", CTRL | ALT | SHIFT, 0)]
    ),
    entry!(
        Action::Copy,
        [Terminal],
        "Terminal",
        "Copy selection",
        &[chord("c", CMD, FN), chord("c", CMD | SHIFT, FN)],
        &[chord("c", CTRL | SHIFT, FN)]
    ),
    entry!(
        Action::Paste,
        [Terminal],
        "Terminal",
        "Paste",
        &[chord("v", CMD, FN), chord("v", CMD | SHIFT, FN)],
        &[chord("v", CTRL | SHIFT, FN)]
    ),
    entry!(
        Action::ScrollTop,
        [Terminal],
        "Terminal (main screen)",
        "Scroll to top",
        &[chord("home", SHIFT, FN)]
    ),
    entry!(
        Action::ScrollBottom,
        [Terminal],
        "Terminal (main screen)",
        "Scroll to bottom",
        &[chord("end", SHIFT, FN)]
    ),
    entry!(
        Action::PageUp,
        [Terminal],
        "Terminal (main screen)",
        "Scroll one page up",
        &[chord("pageup", SHIFT, FN)]
    ),
    entry!(
        Action::PageDown,
        [Terminal],
        "Terminal (main screen)",
        "Scroll one page down",
        &[chord("pagedown", SHIFT, FN)]
    ),
    entry!(
        Action::Rename,
        [Tabs],
        "Focused terminal tab",
        "Rename terminal",
        &[chord("f2", 0, FN)]
    ),
    entry!(
        Action::TabMenu,
        [Tabs],
        "Focused terminal tab",
        "Open terminal menu",
        &[chord("f10", SHIFT, FN)]
    ),
    entry!(
        Action::NextTerminal,
        [Window],
        "Window",
        "Next terminal",
        &[chord("right", CMD, FN)],
        &[chord("f6", CTRL | CMD, FN)]
    ),
    entry!(
        Action::PreviousTerminal,
        [Window],
        "Window",
        "Previous terminal",
        &[chord("left", CMD, FN)],
        &[chord("f6", CTRL | CMD | SHIFT, FN)]
    ),
    entry!("Switch to terminal 1", 1),
    entry!("Switch to terminal 2", 2),
    entry!("Switch to terminal 3", 3),
    entry!("Switch to terminal 4", 4),
    entry!("Switch to terminal 5", 5),
    entry!("Switch to terminal 6", 6),
    entry!("Switch to terminal 7", 7),
    entry!("Switch to terminal 8", 8),
    entry!("Switch to terminal 9", 9),
];

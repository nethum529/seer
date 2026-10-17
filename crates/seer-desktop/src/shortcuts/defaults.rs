use super::*;

macro_rules! entry {
    ($action:ident, [$($scope:ident),+], $context:literal, $name:literal, $mac:expr, $linux:expr) => {
        Shortcut { action: Action::$action, scopes: &[$(Scope::$scope),+], context: $context, name: $name, mac: $mac, linux: $linux }
    };
    ($action:ident, [$($scope:ident),+], $context:literal, $name:literal, $keys:expr) => {
        entry!($action, [$($scope),+], $context, $name, $keys, $keys)
    };
}

pub(crate) const SHORTCUTS: &[Shortcut] = &[
    entry!(
        NewTerminal,
        [Window],
        "Window",
        "New terminal",
        &[chord("n", CMD, 0)],
        &[chord("n", CTRL | SHIFT, 0)]
    ),
    entry!(
        ToggleSidebar,
        [Window],
        "Window",
        "Show or hide sidebar",
        &[chord("b", CMD, 0)],
        &[chord("b", CTRL | SHIFT, 0)]
    ),
    entry!(
        FocusSidebar,
        [Window],
        "Window",
        "Focus sidebar",
        &[chord("b", CMD | SHIFT, 0)],
        &[chord("b", CTRL | ALT | SHIFT, 0)]
    ),
    entry!(
        Copy,
        [Terminal],
        "Terminal",
        "Copy selection",
        &[chord("c", CMD, FN), chord("c", CMD | SHIFT, FN)],
        &[chord("c", CTRL | SHIFT, FN)]
    ),
    entry!(
        Paste,
        [Terminal],
        "Terminal",
        "Paste",
        &[chord("v", CMD, FN), chord("v", CMD | SHIFT, FN)],
        &[chord("v", CTRL | SHIFT, FN)]
    ),
    entry!(
        ScrollTop,
        [Terminal],
        "Terminal (main screen)",
        "Scroll to top",
        &[chord("home", SHIFT, FN)]
    ),
    entry!(
        ScrollBottom,
        [Terminal],
        "Terminal (main screen)",
        "Scroll to bottom",
        &[chord("end", SHIFT, FN)]
    ),
    entry!(
        PageUp,
        [Terminal],
        "Terminal (main screen)",
        "Scroll one page up",
        &[chord("pageup", SHIFT, FN)]
    ),
    entry!(
        PageDown,
        [Terminal],
        "Terminal (main screen)",
        "Scroll one page down",
        &[chord("pagedown", SHIFT, FN)]
    ),
    entry!(
        Rename,
        [Tabs],
        "Focused terminal tab",
        "Rename terminal",
        &[chord("f2", 0, FN)]
    ),
    entry!(
        TabMenu,
        [Tabs],
        "Focused terminal tab",
        "Open terminal menu",
        &[chord("f10", SHIFT, FN)]
    ),
];

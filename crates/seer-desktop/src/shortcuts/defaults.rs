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
    entry!(
        Previous,
        [Tabs],
        "Focused terminal tab",
        "Previous terminal",
        &[
            chord("up", 0, 0),
            chord("up", SHIFT, 0),
            chord("left", 0, 0),
            chord("left", SHIFT, 0)
        ]
    ),
    entry!(
        Next,
        [Tabs],
        "Focused terminal tab",
        "Next terminal",
        &[
            chord("down", 0, 0),
            chord("down", SHIFT, 0),
            chord("right", 0, 0),
            chord("right", SHIFT, 0)
        ]
    ),
    entry!(
        First,
        [Tabs],
        "Focused terminal tab",
        "First terminal",
        &[chord("home", 0, 0), chord("home", SHIFT, 0)]
    ),
    entry!(
        Last,
        [Tabs],
        "Focused terminal tab",
        "Last terminal",
        &[chord("end", 0, 0), chord("end", SHIFT, 0)]
    ),
    entry!(
        Activate,
        [Tabs],
        "Focused terminal tab",
        "Select terminal",
        &[
            chord("enter", 0, 0),
            chord("enter", SHIFT, 0),
            chord("space", 0, 0),
            chord("space", SHIFT, 0)
        ]
    ),
    entry!(
        Activate,
        [Button],
        "Focused button",
        "Activate button",
        &[chord("enter", 0, ALL), chord("space", 0, ALL)]
    ),
    entry!(
        OpenMenu,
        [MenuTrigger],
        "Room or people button",
        "Open menu",
        &[
            chord("enter", 0, ALL),
            chord("space", 0, ALL),
            chord("up", 0, ALL),
            chord("down", 0, ALL)
        ]
    ),
    entry!(
        NextControl,
        [Focus, Settings, Confirmation, Invite],
        "Window and dialogs",
        "Next control",
        &[chord("tab", 0, ALL ^ SHIFT)]
    ),
    entry!(
        PreviousControl,
        [Focus, Settings, Confirmation, Invite],
        "Window and dialogs",
        "Previous control",
        &[chord("tab", SHIFT, ALL ^ SHIFT)]
    ),
    entry!(
        Cancel,
        [Focus, Settings, Confirmation, Invite, TextField],
        "Window and dialogs",
        "Close, cancel, or hide hints",
        &[chord("escape", 0, ALL)]
    ),
    entry!(
        Activate,
        [Confirmation, Invite],
        "Confirmation and invite",
        "Activate focused action",
        &[chord("enter", 0, ALL), chord("space", 0, ALL)]
    ),
    entry!(
        Dismiss,
        [RoomMenu, TabMenu, PeopleMenu],
        "Open menu",
        "Close menu",
        &[chord("escape", 0, ALL), chord("tab", 0, ALL)]
    ),
    entry!(
        Activate,
        [RoomMenu, TabMenu],
        "Room or terminal menu",
        "Activate menu item",
        &[chord("enter", 0, ALL), chord("space", 0, ALL)]
    ),
    entry!(
        Previous,
        [RoomMenu, TabMenu],
        "Room or terminal menu",
        "Previous item",
        &[chord("up", 0, ALL)]
    ),
    entry!(
        Next,
        [RoomMenu, TabMenu],
        "Room or terminal menu",
        "Next item",
        &[chord("down", 0, ALL)]
    ),
    entry!(
        First,
        [RoomMenu, TabMenu, PeopleMenu],
        "Open menu",
        "First item",
        &[chord("home", 0, ALL)]
    ),
    entry!(
        Last,
        [RoomMenu, TabMenu, PeopleMenu],
        "Open menu",
        "Last item",
        &[chord("end", 0, ALL)]
    ),
    entry!(
        Activate,
        [PeopleMenu],
        "People menu",
        "View person's terminals",
        &[chord("enter", 0, ALL)]
    ),
    entry!(
        ShareControl,
        [PeopleMenu],
        "People menu",
        "Share control (others) or view self",
        &[chord("space", 0, ALL)]
    ),
    entry!(
        Previous,
        [PeopleMenu],
        "People menu",
        "Previous person",
        &[chord("up", 0, ALL), chord("left", 0, ALL)]
    ),
    entry!(
        Next,
        [PeopleMenu],
        "People menu",
        "Next person",
        &[chord("down", 0, ALL), chord("right", 0, ALL)]
    ),
    entry!(
        Previous,
        [Invite],
        "Invite duration",
        "Previous duration",
        &[chord("left", 0, ALL)]
    ),
    entry!(
        Next,
        [Invite],
        "Invite duration",
        "Next duration",
        &[chord("right", 0, ALL)]
    ),
    entry!(
        Activate,
        [TextField],
        "Text field",
        "Save or submit",
        &[chord("enter", 0, ALL)]
    ),
    entry!(
        SelectAll,
        [TextField],
        "Text field",
        "Select all text",
        &[chord("a", CMD, ALL ^ CMD), chord("a", CTRL, ALL ^ CTRL)]
    ),
    entry!(
        Copy,
        [TextField],
        "Text field",
        "Copy text",
        &[chord("c", CMD, ALL ^ CMD), chord("c", CTRL, ALL ^ CTRL)]
    ),
    entry!(
        Cut,
        [TextField],
        "Text field",
        "Cut text",
        &[chord("x", CMD, ALL ^ CMD), chord("x", CTRL, ALL ^ CTRL)]
    ),
    entry!(
        Paste,
        [TextField],
        "Text field",
        "Paste text",
        &[chord("v", CMD, ALL ^ CMD), chord("v", CTRL, ALL ^ CTRL)]
    ),
    entry!(
        DeleteBackward,
        [TextField],
        "Text field",
        "Delete previous character",
        &[chord("backspace", 0, ALL)]
    ),
    entry!(
        DeleteForward,
        [TextField],
        "Text field",
        "Delete next character",
        &[chord("delete", 0, ALL)]
    ),
    entry!(
        LineStart,
        [TextField],
        "Text field (Shift selects)",
        "Start of text",
        &[
            chord("home", 0, ALL ^ SHIFT),
            chord("left", CMD, ALL ^ CMD ^ SHIFT),
            chord("home", SHIFT, ALL ^ SHIFT),
            chord("left", CMD | SHIFT, ALL ^ CMD ^ SHIFT)
        ]
    ),
    entry!(
        LineEnd,
        [TextField],
        "Text field (Shift selects)",
        "End of text",
        &[
            chord("end", 0, ALL ^ SHIFT),
            chord("right", CMD, ALL ^ CMD ^ SHIFT),
            chord("end", SHIFT, ALL ^ SHIFT),
            chord("right", CMD | SHIFT, ALL ^ CMD ^ SHIFT)
        ]
    ),
    entry!(
        MoveLeft,
        [TextField],
        "Text field (Shift selects)",
        "Previous character",
        &[
            chord("left", 0, ALL ^ CMD ^ SHIFT),
            chord("left", SHIFT, ALL ^ CMD ^ SHIFT)
        ]
    ),
    entry!(
        MoveRight,
        [TextField],
        "Text field (Shift selects)",
        "Next character",
        &[
            chord("right", 0, ALL ^ CMD ^ SHIFT),
            chord("right", SHIFT, ALL ^ CMD ^ SHIFT)
        ]
    ),
];

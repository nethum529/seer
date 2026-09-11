mod cells;
#[cfg(debug_assertions)]
pub mod debug_log;
pub mod layout;
pub mod proto;
mod terminal;
mod tree;

pub use cells::{Cell, Color};
pub use layout::PaneRect;
pub use terminal::{
    Cursor, CursorShape, InputEvent, KeyCode, KeyInput, Modifiers, MouseButton, MouseInput,
    MouseKind, MouseTracking, TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalFrame,
    TerminalInput, TerminalModes,
};
pub use tree::{
    Layout, LayoutNode, Pane, PaneSize, SplitDirection, Tab, Tree, TreeError, Workspace,
};

#[cfg(debug_assertions)]
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        $crate::debug_log::write(format_args!($($arg)*))
    };
}

#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {};
}

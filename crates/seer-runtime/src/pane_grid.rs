use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell as AlacrittyCell, Flags};
use alacritty_terminal::term::{Config, Term, TermDamage, TermMode};
use alacritty_terminal::vte::ansi::{
    Color as AlacrittyColor, CursorShape as AlacrittyCursorShape, NamedColor, Processor,
};
pub use seer_core::{Cell, Color};
use seer_core::{
    Cursor, CursorShape, InputEvent, KeyCode, KeyInput, Modifiers, MouseKind, MouseTracking,
    TERMINAL_PROTOCOL_VERSION, TerminalFrame, TerminalInput, TerminalModes,
};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::input::{encode_key, encode_mouse};
use crate::pty::lock_mutex;

mod agent_screen;
mod reflow;
mod restore;
mod selection;

const SCROLLBACK_LINES: usize = 1_000;

pub struct PaneGrid {
    terminal: Term<TerminalReplies>,
    parser: Processor,
    replies: TerminalReplies,
    input_changed: bool,
}

impl PaneGrid {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self::with_replies(cols, rows, TerminalReplies::keeping())
    }

    /// A copy that follows the raw stream of a pane. The runtime answers
    /// the queries of the program, so the copy drops its answers.
    pub fn mirror(cols: u16, rows: u16) -> Self {
        Self::with_replies(cols, rows, TerminalReplies::discarding())
    }

    fn with_replies(cols: u16, rows: u16, replies: TerminalReplies) -> Self {
        let dimensions = GridSize::new(cols, rows);
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Config::default()
        };

        Self {
            terminal: Term::new(config, &dimensions, replies.clone()),
            parser: Processor::new(),
            replies,
            input_changed: false,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> bool {
        self.parser.advance(&mut self.terminal, bytes);
        self.parser.sync_bytes_count() == 0
    }

    /// Ends a synchronized update now. True when one was open.
    pub fn end_sync(&mut self) -> bool {
        let open = self.parser.sync_timeout().sync_timeout().is_some();
        if open {
            self.parser.stop_sync(&mut self.terminal);
        }
        open
    }

    /// Bytes that bring a mirror of the same size to this state. End an
    /// open synchronized update first, or its bytes are lost.
    pub fn restore(&mut self) -> Vec<u8> {
        restore::restore(&mut self.terminal)
    }

    #[must_use]
    pub fn snapshot(&self) -> TerminalFrame {
        TerminalFrame {
            rows: (0..self.terminal.screen_lines())
                .map(|row| self.row(row))
                .collect(),
            cursor: self.cursor(),
            modes: self.modes(),
        }
    }

    /// One row of the screen as the snapshot shows it.
    #[must_use]
    pub fn row(&self, row: usize) -> Vec<Cell> {
        let grid = self.terminal.grid();
        let line = Line(row as i32 - display_offset(grid));
        (0..grid.columns())
            .map(|column| map_cell(&grid[line][Column(column)]))
            .collect()
    }

    /// The screen rows that changed since the last call. None means every
    /// row.
    pub fn take_damage(&mut self) -> Option<Vec<usize>> {
        let rows = match self.terminal.damage() {
            TermDamage::Full => None,
            TermDamage::Partial(lines) => Some(lines.map(|bounds| bounds.line).collect()),
        };
        self.terminal.reset_damage();
        rows
    }

    /// The screen as a viewer of another size sees it. `None` means the
    /// plain snapshot is right: the size matches, or a full screen app
    /// draws for the one PTY size and cannot be wrapped (issue 406).
    #[must_use]
    pub fn view(&self, cols: u16, rows: u16) -> Option<TerminalFrame> {
        let grid = self.terminal.grid();
        let mode = self.terminal.mode();
        if cols == 0
            || rows == 0
            || mode.contains(TermMode::ALT_SCREEN)
            || (usize::from(cols), usize::from(rows)) == (grid.columns(), grid.screen_lines())
        {
            return None;
        }
        let view = reflow::view(grid, usize::from(cols), usize::from(rows));
        Some(TerminalFrame {
            rows: view.rows,
            cursor: self.cursor_at(view.cursor),
            modes: self.modes(),
        })
    }

    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        let grid = self.terminal.grid();
        (to_u16(grid.columns()), to_u16(grid.screen_lines()))
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.terminal.resize(GridSize::new(cols, rows));
    }

    /// Bytes the terminal asked to write back to the pty master.
    ///
    /// A shell can query terminal capabilities. Alacritty answers
    /// those queries by emitting a PtyWrite event. The pane host
    /// sends the captured bytes back into the pty.
    pub(crate) fn take_replies(&self) -> Vec<u8> {
        self.replies.take()
    }

    pub fn handle_input(&mut self, input: &TerminalInput) -> io::Result<Option<Vec<u8>>> {
        if input.protocol_version != TERMINAL_PROTOCOL_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported terminal input version",
            ));
        }
        let mode = *self.terminal.mode();
        match &input.event {
            InputEvent::Key(key) => Ok(Some(encode_key(*key, mode))),
            InputEvent::Text(text) => Ok(Some(text.as_bytes().to_vec())),
            InputEvent::Paste(text) => Ok(Some(encode_paste(text, mode))),
            InputEvent::Mouse(mouse) => Ok(self.handle_mouse(*mouse, mode)),
            InputEvent::Focus(focused) => Ok(focus_bytes(*focused, mode)),
            InputEvent::Scrollback { lines } => {
                self.scroll(*lines, mode);
                Ok(None)
            }
        }
    }

    pub(crate) fn take_input_changed(&mut self) -> bool {
        std::mem::take(&mut self.input_changed)
    }

    pub(crate) fn end_expired_sync(&mut self) -> bool {
        let expired = self
            .parser
            .sync_timeout()
            .sync_timeout()
            .is_some_and(|timeout| timeout <= Instant::now());
        if expired {
            self.parser.stop_sync(&mut self.terminal);
        }
        expired
    }

    #[must_use]
    pub fn cursor(&self) -> Cursor {
        let point = self.terminal.grid().cursor.point;
        let row = point.line.0 + display_offset(self.terminal.grid());
        let on_screen = row >= 0 && row < self.terminal.screen_lines() as i32;
        self.cursor_at(on_screen.then_some((row as u16, point.column.0 as u16)))
    }

    fn cursor_at(&self, position: Option<(u16, u16)>) -> Cursor {
        let style = self.terminal.cursor_style();
        let (row, column) = position.unwrap_or_default();
        Cursor {
            row,
            column,
            shape: map_cursor_shape(style.shape),
            blinking: style.blinking,
            visible: position.is_some()
                && self.terminal.mode().contains(TermMode::SHOW_CURSOR)
                && style.shape != AlacrittyCursorShape::Hidden,
        }
    }

    pub fn modes(&self) -> TerminalModes {
        let mode = self.terminal.mode();
        TerminalModes {
            mouse_tracking: mouse_tracking(*mode),
            alt_screen: mode.contains(TermMode::ALT_SCREEN),
        }
    }

    #[must_use]
    pub fn alternate_scroll(&self) -> bool {
        self.terminal
            .mode()
            .contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
    }

    fn handle_mouse(&mut self, mouse: seer_core::MouseInput, mode: TermMode) -> Option<Vec<u8>> {
        if mode.intersects(TermMode::MOUSE_MODE) {
            return encode_mouse(mouse, mode);
        }
        match mouse.kind {
            MouseKind::ScrollUp => self.scroll_or_arrow(3, mode),
            MouseKind::ScrollDown => self.scroll_or_arrow(-3, mode),
            MouseKind::Down
            | MouseKind::Up
            | MouseKind::Drag
            | MouseKind::Moved
            | MouseKind::ScrollLeft
            | MouseKind::ScrollRight => None,
        }
    }

    fn scroll_or_arrow(&mut self, lines: i32, mode: TermMode) -> Option<Vec<u8>> {
        if mode.contains(TermMode::ALT_SCREEN) {
            let code = if lines > 0 {
                KeyCode::Up
            } else {
                KeyCode::Down
            };
            Some(encode_key(
                KeyInput {
                    code,
                    modifiers: Modifiers::default(),
                },
                mode,
            ))
        } else {
            self.scroll(lines, mode);
            None
        }
    }

    fn scroll(&mut self, lines: i32, mode: TermMode) {
        if lines != 0 && !mode.contains(TermMode::ALT_SCREEN) {
            let before = self.terminal.grid().display_offset();
            self.terminal.scroll_display(Scroll::Delta(lines));
            self.input_changed = before != self.terminal.grid().display_offset();
        }
    }
}

/// Captures terminal reply events that must go back to the pty master.
#[derive(Clone)]
struct TerminalReplies {
    bytes: Option<Arc<Mutex<Vec<u8>>>>,
    title: Arc<Mutex<String>>,
}

impl TerminalReplies {
    fn keeping() -> Self {
        Self {
            bytes: Some(Arc::default()),
            title: Arc::default(),
        }
    }

    fn discarding() -> Self {
        Self {
            bytes: None,
            title: Arc::default(),
        }
    }

    fn take(&self) -> Vec<u8> {
        self.bytes
            .as_ref()
            .map(|bytes| std::mem::take(&mut *lock_mutex(bytes)))
            .unwrap_or_default()
    }
}

impl EventListener for TerminalReplies {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(reply) => {
                if let Some(bytes) = &self.bytes {
                    lock_mutex(bytes).extend(reply.into_bytes());
                }
            }
            Event::Title(title) => *lock_mutex(&self.title) = title.chars().take(512).collect(),
            Event::ResetTitle => lock_mutex(&self.title).clear(),
            _ => {}
        }
    }
}

fn display_offset(grid: &alacritty_terminal::Grid<AlacrittyCell>) -> i32 {
    grid.display_offset().min(grid.history_size()) as i32
}

fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn encode_paste(text: &str, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        let text = text.replace(['\x1b', '\x03'], "");
        [b"\x1b[200~".as_slice(), text.as_bytes(), b"\x1b[201~"].concat()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

fn focus_bytes(focused: bool, mode: TermMode) -> Option<Vec<u8>> {
    mode.contains(TermMode::FOCUS_IN_OUT)
        .then(|| if focused { b"\x1b[I" } else { b"\x1b[O" }.to_vec())
}

fn mouse_tracking(mode: TermMode) -> MouseTracking {
    if mode.contains(TermMode::MOUSE_MOTION) {
        MouseTracking::AnyMotion
    } else if mode.contains(TermMode::MOUSE_DRAG) {
        MouseTracking::ButtonMotion
    } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
        MouseTracking::Click
    } else {
        MouseTracking::None
    }
}

fn map_cursor_shape(shape: AlacrittyCursorShape) -> CursorShape {
    match shape {
        AlacrittyCursorShape::Block | AlacrittyCursorShape::Hidden => CursorShape::Block,
        AlacrittyCursorShape::Underline => CursorShape::Underline,
        AlacrittyCursorShape::Beam => CursorShape::Beam,
        AlacrittyCursorShape::HollowBlock => CursorShape::HollowBlock,
    }
}

fn map_cell(cell: &AlacrittyCell) -> Cell {
    Cell {
        character: cell.c,
        fg: map_color(cell.fg),
        bg: map_color(cell.bg),
        bold: cell.flags.contains(Flags::BOLD),
        italic: cell.flags.contains(Flags::ITALIC),
        underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
        dim: cell.flags.contains(Flags::DIM),
        inverse: cell.flags.contains(Flags::INVERSE),
        hidden: cell.flags.contains(Flags::HIDDEN),
        strikeout: cell.flags.contains(Flags::STRIKEOUT),
    }
}

fn map_color(color: AlacrittyColor) -> Color {
    match color {
        AlacrittyColor::Spec(rgb) => Color::Rgb {
            red: rgb.r,
            green: rgb.g,
            blue: rgb.b,
        },
        AlacrittyColor::Indexed(index) => Color::Indexed(index),
        AlacrittyColor::Named(name) if name <= NamedColor::BrightWhite => {
            Color::Indexed(name as u8)
        }
        AlacrittyColor::Named(_) => Color::Default,
    }
}

#[derive(Clone, Copy)]
struct GridSize {
    cols: usize,
    rows: usize,
}

impl GridSize {
    fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols: usize::from(cols),
            rows: usize::from(rows),
        }
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Alacritty grows display_offset on a scroll inside a region while a
    // viewer is scrolled back, but it caps it at the scrollback limit, not
    // at the rows that exist. The snapshot must show the oldest row that
    // exists at the top, not a row past it and not a row below it.
    #[test]
    fn a_viewer_scrolled_past_the_history_sees_the_oldest_row_at_the_top() {
        let long = b"line of text that is quite long and will wrap around the edge";
        let mut grid = PaneGrid::new(20, 6);
        grid.feed(long);
        grid.resize(14, 8);
        grid.handle_input(&TerminalInput::new(InputEvent::Scrollback { lines: 1 }))
            .expect("scrollback is valid input");
        grid.feed(b"\x1b[3M");
        grid.feed(long);

        let snapshot = grid.snapshot();
        let top: String = snapshot.rows[0].iter().map(|cell| cell.character).collect();
        assert_eq!(top, "line of text t");
    }
}

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell as AlacrittyCell, Flags};
use alacritty_terminal::term::{Config, Term, TermMode};
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

const SCROLLBACK_LINES: usize = 1_000;

pub struct PaneGrid {
    terminal: Term<TerminalReplies>,
    parser: Processor,
    replies: TerminalReplies,
    input_changed: bool,
}

impl PaneGrid {
    pub fn new(cols: u16, rows: u16) -> Self {
        let dimensions = GridSize::new(cols, rows);
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Config::default()
        };
        let replies = TerminalReplies::default();

        Self {
            terminal: Term::new(config, &dimensions, replies.clone()),
            parser: Processor::new(),
            replies,
            input_changed: false,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return self.finish_expired_sync();
        }
        self.parser.advance(&mut self.terminal, bytes);
        self.parser.sync_bytes_count() == 0
    }

    #[must_use]
    pub fn snapshot(&self) -> TerminalFrame {
        let grid = self.terminal.grid();
        let display_offset = grid.display_offset() as i32;
        let rows = (0..grid.screen_lines())
            .map(|row| {
                let line = Line(row as i32 - display_offset);
                (0..grid.columns())
                    .map(|column| map_cell(&grid[line][Column(column)]))
                    .collect()
            })
            .collect();
        TerminalFrame {
            rows,
            cursor: self.cursor(display_offset),
            modes: self.modes(),
        }
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

    fn finish_expired_sync(&mut self) -> bool {
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

    fn cursor(&self, display_offset: i32) -> Cursor {
        let point = self.terminal.grid().cursor.point;
        let row = point.line.0 + display_offset;
        let style = self.terminal.cursor_style();
        Cursor {
            row: row.max(0) as u16,
            column: point.column.0 as u16,
            shape: map_cursor_shape(style.shape),
            blinking: style.blinking,
            visible: self.terminal.mode().contains(TermMode::SHOW_CURSOR)
                && row >= 0
                && row < self.terminal.screen_lines() as i32
                && style.shape != AlacrittyCursorShape::Hidden,
        }
    }

    fn modes(&self) -> TerminalModes {
        let mode = self.terminal.mode();
        TerminalModes {
            mouse_tracking: mouse_tracking(*mode),
        }
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
#[derive(Clone, Default)]
struct TerminalReplies {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl TerminalReplies {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *lock_mutex(&self.bytes))
    }
}

impl EventListener for TerminalReplies {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(reply) = event {
            lock_mutex(&self.bytes).extend(reply.into_bytes());
        }
    }
}

fn encode_paste(text: &str, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        [b"\x1b[200~".as_slice(), text.as_bytes(), b"\x1b[201~"].concat()
    } else {
        text.as_bytes().to_vec()
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

    #[test]
    fn captures_terminal_replies_until_taken() {
        let replies = TerminalReplies::default();

        replies.send_event(Event::PtyWrite("first".to_owned()));
        replies.send_event(Event::PtyWrite("second".to_owned()));

        assert_eq!(replies.take(), b"firstsecond".to_vec());
        assert!(replies.take().is_empty());
    }

    #[test]
    fn feeds_plain_text() {
        let mut grid = PaneGrid::new(5, 2);

        grid.feed(b"hello");

        let snapshot = grid.snapshot();
        let first_row: String = snapshot.rows[0].iter().map(|cell| cell.character).collect();
        assert_eq!(first_row, "hello");
        assert_eq!(snapshot.rows[1][0].character, ' ');
        assert_eq!(
            snapshot.rows[0][0],
            Cell {
                character: 'h',
                fg: Color::Default,
                bg: Color::Default,
                bold: false,
                italic: false,
                underline: false,
                dim: false,
                inverse: false,
                hidden: false,
                strikeout: false,
            }
        );
    }

    #[test]
    fn feeds_sgr_colors_and_flags() {
        let mut grid = PaneGrid::new(3, 1);

        grid.feed(b"\x1b[1;2;3;4;7;8;9;31;48;5;123mX\x1b[0;38;2;10;20;30mY");

        let snapshot = grid.snapshot();
        let styled = &snapshot.rows[0][0];
        assert_eq!(styled.fg, Color::Indexed(1));
        assert_eq!(styled.bg, Color::Indexed(123));
        assert!(styled.bold);
        assert!(styled.italic);
        assert!(styled.underline);
        assert!(styled.dim);
        assert!(styled.inverse);
        assert!(styled.hidden);
        assert!(styled.strikeout);
        assert_eq!(
            snapshot.rows[0][1].fg,
            Color::Rgb {
                red: 10,
                green: 20,
                blue: 30,
            }
        );
    }

    #[test]
    fn feeds_cursor_move() {
        let mut grid = PaneGrid::new(4, 3);

        grid.feed(b"\x1b[2;3HZ");

        let snapshot = grid.snapshot();
        assert_eq!(snapshot.rows[1][2].character, 'Z');
        assert_eq!(snapshot.rows[0][0].character, ' ');
    }

    #[test]
    fn resizes_grid_shape() {
        let mut grid = PaneGrid::new(2, 2);

        grid.resize(3, 4);

        let snapshot = grid.snapshot();
        assert_eq!(snapshot.rows.len(), 4);
        assert!(snapshot.rows.iter().all(|row| row.len() == 3));
    }
}

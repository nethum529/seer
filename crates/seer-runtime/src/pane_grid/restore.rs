use alacritty_terminal::grid::{Charsets, Cursor, Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{
    CharsetIndex, Color, CursorShape, CursorStyle, Handler, StandardCharset,
};
use std::io::Write;

use super::TerminalReplies;

const STYLE: Flags = Flags::BOLD
    .union(Flags::DIM)
    .union(Flags::ITALIC)
    .union(Flags::ALL_UNDERLINES)
    .union(Flags::INVERSE)
    .union(Flags::HIDDEN)
    .union(Flags::STRIKEOUT);

// Private DEC modes that a fresh terminal has off, and the ones it has on.
const SET_MODES: [(TermMode, u16); 9] = [
    (TermMode::APP_CURSOR, 1),
    (TermMode::ORIGIN, 6),
    (TermMode::MOUSE_REPORT_CLICK, 1000),
    (TermMode::MOUSE_DRAG, 1002),
    (TermMode::MOUSE_MOTION, 1003),
    (TermMode::FOCUS_IN_OUT, 1004),
    (TermMode::UTF8_MOUSE, 1005),
    (TermMode::SGR_MOUSE, 1006),
    (TermMode::BRACKETED_PASTE, 2004),
];
const RESET_MODES: [(TermMode, u16); 4] = [
    (TermMode::SHOW_CURSOR, 25),
    (TermMode::LINE_WRAP, 7),
    (TermMode::ALTERNATE_SCROLL, 1007),
    (TermMode::URGENCY_HINTS, 1042),
];

const SETS: [(CharsetIndex, u8); 4] = [
    (CharsetIndex::G0, b'('),
    (CharsetIndex::G1, b')'),
    (CharsetIndex::G2, b'*'),
    (CharsetIndex::G3, b'+'),
];

/// Bytes that bring a new terminal of the same size to the state of `term`:
/// the scrollback, the screen, the screen under a full screen app, the
/// modes, the character sets, the cursor and the pen. The scroll region,
/// the tab stops and the title are private in alacritty_terminal 0.26, so a
/// new terminal gets the defaults.
pub(super) fn restore(term: &mut Term<TerminalReplies>) -> Vec<u8> {
    let shifted = shifted_out(term);
    let ascii = Charsets::default();
    let mut out = Vec::new();
    if term.mode().contains(TermMode::ALT_SCREEN) {
        // From the alt screen, swap_alt only swaps. The swap back clears the
        // alt screen and sets the saved cursor of the primary screen to its
        // cursor, which 1049h already did. The clone undoes the clear.
        let alt = term.grid().clone();
        term.swap_alt();
        let primary = term.grid().cursor.charsets;
        lines(&mut out, term.grid());
        place(&mut out, term.grid(), &term.grid().cursor);
        designate(&mut out, ascii, primary);
        term.swap_alt();
        *term.grid_mut() = alt;
        out.extend_from_slice(b"\x1b[?1049h\x1b[H");
        designate(&mut out, primary, ascii);
    }
    let grid = term.grid();
    lines(&mut out, grid);
    modes(&mut out, *term.mode(), term.cursor_style());
    place(&mut out, grid, &grid.saved_cursor);
    designate(&mut out, ascii, grid.saved_cursor.charsets);
    out.extend_from_slice(b"\x1b7");
    designate(&mut out, grid.saved_cursor.charsets, ascii);
    place(&mut out, grid, &grid.cursor);
    designate(&mut out, ascii, grid.cursor.charsets);
    if shifted {
        out.push(0x0e);
    }
    out
}

// alacritty_terminal 0.26 keeps private which set (G0 after SI, G1 after
// SO) maps the output. Make G1 differ from G0, print one probe character,
// read the cell, and put the cursor (with its sets) and the rows that the
// print can change back.
fn shifted_out(term: &mut Term<TerminalReplies>) -> bool {
    let cursor = term.grid().cursor.clone();
    let probe = match cursor.charsets[CharsetIndex::G0] {
        StandardCharset::Ascii => StandardCharset::SpecialCharacterAndLineDrawing,
        StandardCharset::SpecialCharacterAndLineDrawing => StandardCharset::Ascii,
    };
    term.configure_charset(CharsetIndex::G1, probe);
    let line = cursor.point.line;
    let rows: Vec<_> = [line - 1, line]
        .into_iter()
        .filter(|row| *row >= term.grid().topmost_line())
        .map(|row| (row, term.grid()[row].clone()))
        .collect();
    term.grid_mut().cursor.input_needs_wrap = false;
    term.input('q');
    let shown = term.grid()[line][cursor.point.column].c;
    for (row, cells) in rows {
        term.grid_mut()[row] = cells;
    }
    term.grid_mut().cursor = cursor;
    shown == probe.map('q')
}

fn designate(out: &mut Vec<u8>, from: Charsets, to: Charsets) {
    for (index, slot) in SETS {
        if from[index] != to[index] {
            let set = match to[index] {
                StandardCharset::Ascii => b'B',
                StandardCharset::SpecialCharacterAndLineDrawing => b'0',
            };
            out.extend_from_slice(&[0x1b, slot, set]);
        }
    }
}

fn lines(out: &mut Vec<u8>, grid: &Grid<Cell>) {
    let columns = grid.columns();
    let bottom = grid.screen_lines() as i32;
    let mut pen = Pen::default();
    for line in -(grid.history_size() as i32)..bottom {
        let row = &grid[Line(line)];
        let wrapped = row[Column(columns - 1)].flags.contains(Flags::WRAPLINE);
        let end = if wrapped {
            columns
        } else {
            (0..columns)
                .rposition(|column| row[Column(column)] != Cell::default())
                .map_or(0, |last| last + 1)
        };
        for column in 0..end {
            let cell = &row[Column(column)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            pen.set(out, cell);
            put(out, cell);
            // A tab marks only its first cell. The tab stops may differ, so
            // the cursor goes to the next cell by column.
            if cell.c == '\t' {
                let _ = write!(out, "\x1b[{}G", column + 2);
            }
        }
        if line + 1 < bottom && !wrapped {
            // A new line from a scroll takes the background of the pen.
            pen.reset(out);
            out.extend_from_slice(b"\r\n");
        }
    }
    pen.reset(out);
}

fn put(out: &mut Vec<u8>, cell: &Cell) {
    let mut buffer = [0; 4];
    out.extend_from_slice(cell.c.encode_utf8(&mut buffer).as_bytes());
    for mark in cell.zerowidth().unwrap_or_default() {
        out.extend_from_slice(mark.encode_utf8(&mut buffer).as_bytes());
    }
}

fn modes(out: &mut Vec<u8>, mode: TermMode, style: CursorStyle) {
    for (flag, number) in SET_MODES {
        if mode.contains(flag) {
            let _ = write!(out, "\x1b[?{number}h");
        }
    }
    for (flag, number) in RESET_MODES {
        if !mode.contains(flag) {
            let _ = write!(out, "\x1b[?{number}l");
        }
    }
    if mode.contains(TermMode::APP_KEYPAD) {
        out.extend_from_slice(b"\x1b=");
    }
    if mode.contains(TermMode::INSERT) {
        out.extend_from_slice(b"\x1b[4h");
    }
    if mode.contains(TermMode::LINE_FEED_NEW_LINE) {
        out.extend_from_slice(b"\x1b[20h");
    }
    let shape = match style.shape {
        CursorShape::Block => 1,
        CursorShape::Underline => 3,
        CursorShape::Beam => 5,
        CursorShape::HollowBlock | CursorShape::Hidden => return,
    };
    let _ = write!(out, "\x1b[{} q", shape + u8::from(!style.blinking));
}

// A cursor after the last column waits to wrap. Writing the last cell
// again puts a new terminal in the same state.
fn place(out: &mut Vec<u8>, grid: &Grid<Cell>, cursor: &Cursor<Cell>) {
    let line = cursor.point.line;
    let mut column = cursor.point.column.0;
    let row = &grid[line];
    if cursor.input_needs_wrap && row[Column(column)].flags.contains(Flags::WIDE_CHAR_SPACER) {
        column = column.saturating_sub(1);
    }
    let _ = write!(out, "\x1b[{};{}H\x1b[0m", line.0 + 1, column + 1);
    let mut pen = Pen::default();
    if cursor.input_needs_wrap {
        pen.set(out, &row[Column(column)]);
        put(out, &row[Column(column)]);
    }
    pen.set(out, &cursor.template);
}

type Style = (Color, Color, Flags, Option<Color>);

// The style the terminal writes with. None is the default style.
#[derive(Default)]
struct Pen(Option<Style>);

impl Pen {
    fn set(&mut self, out: &mut Vec<u8>, cell: &Cell) {
        let style = (cell.fg, cell.bg, cell.flags & STYLE, cell.underline_color());
        let blank = Cell::default();
        let current = self
            .0
            .unwrap_or((blank.fg, blank.bg, blank.flags, blank.underline_color()));
        if current == style {
            return;
        }
        out.extend_from_slice(b"\x1b[0");
        for (flag, code) in [
            (Flags::BOLD, "1"),
            (Flags::DIM, "2"),
            (Flags::ITALIC, "3"),
            (Flags::UNDERLINE, "4"),
            (Flags::DOUBLE_UNDERLINE, "4:2"),
            (Flags::UNDERCURL, "4:3"),
            (Flags::DOTTED_UNDERLINE, "4:4"),
            (Flags::DASHED_UNDERLINE, "4:5"),
            (Flags::INVERSE, "7"),
            (Flags::HIDDEN, "8"),
            (Flags::STRIKEOUT, "9"),
        ] {
            if style.2.contains(flag) {
                let _ = write!(out, ";{code}");
            }
        }
        color(out, style.0, 30, 90, 38);
        color(out, style.1, 40, 100, 48);
        if let Some(underline) = style.3 {
            color(out, underline, 0, 0, 58);
        }
        out.push(b'm');
        self.0 = Some(style);
    }

    fn reset(&mut self, out: &mut Vec<u8>) {
        if self.0.take().is_some() {
            out.extend_from_slice(b"\x1b[0m");
        }
    }
}

fn color(out: &mut Vec<u8>, color: Color, normal: u8, bright: u8, extended: u8) {
    let _ = match color {
        Color::Named(name) if (name as usize) < 8 && normal != 0 => {
            write!(out, ";{}", normal + name as u8)
        }
        Color::Named(name) if (name as usize) < 16 && bright != 0 => {
            write!(out, ";{}", bright + name as u8 - 8)
        }
        Color::Named(name) if (name as usize) < 16 => {
            write!(out, ";{extended};5;{}", name as u8)
        }
        Color::Named(_) => Ok(()),
        Color::Indexed(index) => write!(out, ";{extended};5;{index}"),
        Color::Spec(rgb) => write!(out, ";{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b),
    };
}

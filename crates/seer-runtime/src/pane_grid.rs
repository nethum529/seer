use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell as AlacrittyCell, Flags};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Color as AlacrittyColor, NamedColor, Processor};
pub use seer_core::{Cell, Color};

const SCROLLBACK_LINES: usize = 1_000;

pub struct PaneGrid {
    terminal: Term<VoidListener>,
    parser: Processor,
}

impl PaneGrid {
    pub fn new(cols: u16, rows: u16) -> Self {
        let dimensions = GridSize::new(cols, rows);
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Config::default()
        };

        Self {
            terminal: Term::new(config, &dimensions, VoidListener),
            parser: Processor::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.terminal, bytes);
    }

    pub fn snapshot(&self) -> Vec<Vec<Cell>> {
        let grid = self.terminal.grid();
        let display_offset = grid.display_offset() as i32;

        (0..grid.screen_lines())
            .map(|row| {
                let line = Line(row as i32 - display_offset);
                (0..grid.columns())
                    .map(|column| map_cell(&grid[line][Column(column)]))
                    .collect()
            })
            .collect()
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.terminal.resize(GridSize::new(cols, rows));
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
    fn feeds_plain_text() {
        let mut grid = PaneGrid::new(5, 2);

        grid.feed(b"hello");

        let snapshot = grid.snapshot();
        let first_row: String = snapshot[0].iter().map(|cell| cell.character).collect();
        assert_eq!(first_row, "hello");
        assert_eq!(snapshot[1][0].character, ' ');
        assert_eq!(
            snapshot[0][0],
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
        let styled = &snapshot[0][0];
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
            snapshot[0][1].fg,
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
        assert_eq!(snapshot[1][2].character, 'Z');
        assert_eq!(snapshot[0][0].character, ' ');
    }

    #[test]
    fn resizes_grid_shape() {
        let mut grid = PaneGrid::new(2, 2);

        grid.resize(3, 4);

        let snapshot = grid.snapshot();
        assert_eq!(snapshot.len(), 4);
        assert!(snapshot.iter().all(|row| row.len() == 3));
    }
}

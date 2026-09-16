use crate::theme::Palette;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Borders, Widget},
};
use seer_core::Cell;

/// Draws a screen into `area` and returns the rect the screen took.
///
/// A full screen app draws for the one PTY size and cannot be wrapped.
/// When it is smaller than the area it sits in the middle with an edge
/// around it. Everything else fills the area from the top left corner.
pub(crate) fn draw_screen(
    frame: &mut Frame<'_>,
    rows: &[Vec<Cell>],
    fixed: bool,
    area: Rect,
) -> Rect {
    let width = rows
        .first()
        .map_or(0, Vec::len)
        .min(usize::from(area.width)) as u16;
    let height = rows.len().min(usize::from(area.height)) as u16;
    if !fixed || rows.is_empty() || (width, height) == (area.width, area.height) {
        frame.render_widget(PaneCells::new(rows), area);
        return area;
    }
    let placed = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    edge(frame, placed, area);
    frame.render_widget(PaneCells::new(rows), placed);
    placed
}

fn edge(frame: &mut Frame<'_>, placed: Rect, area: Rect) {
    let mut borders = Borders::NONE;
    let mut outer = placed;
    if placed.x > area.x {
        outer.x -= 1;
        outer.width += 1;
        borders |= Borders::LEFT;
    }
    if placed.right() < area.right() {
        outer.width += 1;
        borders |= Borders::RIGHT;
    }
    if placed.y > area.y {
        outer.y -= 1;
        outer.height += 1;
        borders |= Borders::TOP;
    }
    if placed.bottom() < area.bottom() {
        outer.height += 1;
        borders |= Borders::BOTTOM;
    }
    frame.render_widget(Palette::default().block(false).borders(borders), outer);
}

pub(crate) struct PaneCells<'a> {
    rows: &'a [Vec<Cell>],
}

impl<'a> PaneCells<'a> {
    pub(crate) fn new(rows: &'a [Vec<Cell>]) -> Self {
        Self { rows }
    }
}

impl Widget for PaneCells<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Style::default().fg(Color::Reset).bg(Color::Reset));
        for (row_index, row) in self.rows.iter().take(area.height as usize).enumerate() {
            let y = area.y.saturating_add(row_index as u16);
            for (column_index, cell) in row.iter().take(area.width as usize).enumerate() {
                let x = area.x.saturating_add(column_index as u16);
                buffer[(x, y)]
                    .set_char(cell.character)
                    .set_style(cell_style(cell));
            }
        }
    }
}

fn cell_style(cell: &Cell) -> Style {
    let mut modifiers = Modifier::empty();
    modifiers.set(Modifier::BOLD, cell.bold);
    modifiers.set(Modifier::ITALIC, cell.italic);
    modifiers.set(Modifier::UNDERLINED, cell.underline);
    modifiers.set(Modifier::DIM, cell.dim);
    modifiers.set(Modifier::REVERSED, cell.inverse);
    modifiers.set(Modifier::HIDDEN, cell.hidden);
    modifiers.set(Modifier::CROSSED_OUT, cell.strikeout);
    Style::default()
        .fg(terminal_color(cell.fg))
        .bg(terminal_color(cell.bg))
        .add_modifier(modifiers)
}

fn terminal_color(color: seer_core::Color) -> Color {
    match color {
        seer_core::Color::Default => Color::Reset,
        seer_core::Color::Indexed(index) => Color::Indexed(index),
        seer_core::Color::Rgb { red, green, blue } => Color::Rgb(red, green, blue),
    }
}

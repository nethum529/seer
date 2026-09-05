use crate::theme::Palette;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};
use seer_core::Cell;

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
        let palette = Palette::default();
        buffer.set_style(area, palette.style());
        let start = start_row(self.rows, area.height);
        for (row_index, row) in self
            .rows
            .iter()
            .skip(start)
            .take(area.height as usize)
            .enumerate()
        {
            let y = area.y.saturating_add(row_index as u16);
            for (column_index, cell) in row.iter().take(area.width as usize).enumerate() {
                let x = area.x.saturating_add(column_index as u16);
                buffer[(x, y)]
                    .set_char(cell.character)
                    .set_style(cell_style(cell, palette));
            }
        }
    }
}

pub(crate) fn start_row(rows: &[Vec<Cell>], height: u16) -> usize {
    content_rows(rows).saturating_sub(usize::from(height))
}

fn content_rows(rows: &[Vec<Cell>]) -> usize {
    rows.iter()
        .rposition(|row| row.iter().any(is_visible))
        .map_or(0, |index| index + 1)
}

fn is_visible(cell: &Cell) -> bool {
    cell.character != ' ' || cell.bg != seer_core::Color::Default || cell.inverse
}

fn cell_style(cell: &Cell, palette: Palette) -> Style {
    let mut modifiers = Modifier::empty();
    modifiers.set(Modifier::BOLD, cell.bold);
    modifiers.set(Modifier::ITALIC, cell.italic);
    modifiers.set(Modifier::UNDERLINED, cell.underline);
    modifiers.set(Modifier::DIM, cell.dim);
    modifiers.set(Modifier::REVERSED, cell.inverse);
    modifiers.set(Modifier::HIDDEN, cell.hidden);
    modifiers.set(Modifier::CROSSED_OUT, cell.strikeout);
    Style::default()
        .fg(palette.terminal_color(cell.fg, palette.text))
        .bg(palette.terminal_color(cell.bg, Color::Reset))
        .add_modifier(modifiers)
}

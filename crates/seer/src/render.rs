use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;
use seer_core::{Cell, Color};

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
        .fg(color(cell.fg))
        .bg(color(cell.bg))
        .add_modifier(modifiers)
}

fn color(color: Color) -> ratatui::style::Color {
    match color {
        Color::Default => ratatui::style::Color::Reset,
        Color::Indexed(index) => ratatui::style::Color::Indexed(index),
        Color::Rgb { red, green, blue } => ratatui::style::Color::Rgb(red, green, blue),
    }
}

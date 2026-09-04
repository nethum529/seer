use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use seer_core::{Cell, Color};

use crate::state::{ClientState, pane_rects};

pub(crate) fn draw_tree(
    frame: &mut Frame<'_>,
    state: &mut ClientState,
    mut area: Rect,
    status: &str,
    peek_person: Option<&str>,
) {
    let status_height = area.height.min(1);
    let status_area = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(status_height),
        area.width,
        status_height,
    );
    frame.render_widget(Paragraph::new(status), status_area);
    area.height = area.height.saturating_sub(status_height);
    if let Some(person) = peek_person {
        let banner_height = area.height.min(2);
        let banner = Rect::new(area.x, area.y, area.width, banner_height);
        frame.render_widget(
            Paragraph::new(format!(
                "PEEK: {person} - READ ONLY\nWorkspace: {person}/{}",
                state.selected_workspace().unwrap_or("unknown")
            )),
            banner,
        );
        area.y = area.y.saturating_add(banner_height);
        area.height = area.height.saturating_sub(banner_height);
    }
    draw_panes(frame, state, area);
}

fn draw_panes(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    let Some(tab) = state.visible_tab().cloned() else {
        state.set_pane_areas(Vec::new());
        return;
    };
    let mut input_areas = Vec::new();
    for (pane, pane_area) in pane_rects(&tab, area) {
        let block = Block::default().borders(Borders::ALL).title(pane.as_str());
        let inner = block.inner(pane_area);
        frame.render_widget(block, pane_area);
        frame.render_widget(PaneCells::new(state.pane_rows(&pane)), inner);
        set_frame_cursor(frame, state, &pane, inner);
        input_areas.push((pane, inner));
    }
    state.set_pane_areas(input_areas);
}

fn set_frame_cursor(frame: &mut Frame<'_>, state: &ClientState, pane: &str, area: Rect) {
    let Some(cursor) = state
        .pane_cursor(pane)
        .filter(|cursor| cursor.visible && state.focused() == Some(pane))
    else {
        return;
    };
    if cursor.column < area.width && cursor.row < area.height {
        frame.set_cursor_position((area.x + cursor.column, area.y + cursor.row));
    }
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

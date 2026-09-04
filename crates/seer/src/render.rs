use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use seer_core::{Cell, Color};

use crate::state::{ClientState, pane_rects};
use crate::theme::Palette;

pub(crate) fn draw_tree(
    frame: &mut Frame<'_>,
    state: &mut ClientState,
    mut area: Rect,
    status: &str,
    peek_person: Option<&str>,
) {
    let palette = Palette::default();
    frame.render_widget(Paragraph::new("").style(palette.style()), area);
    let status_height = area.height.min(1);
    let status_area = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(status_height),
        area.width,
        status_height,
    );
    draw_footer(frame, status_area, status);
    area.height = area.height.saturating_sub(status_height);
    if peek_person.is_none() {
        let top = Rect::new(area.x, area.y, area.width, area.height.min(1));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " seer",
                palette
                    .style()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            )))
            .style(palette.style()),
            top,
        );
        area.y = area.y.saturating_add(top.height);
        area.height = area.height.saturating_sub(top.height);
    }
    if let Some(person) = peek_person {
        let banner_height = area.height.min(2);
        let banner = Rect::new(area.x, area.y, area.width, banner_height);
        frame.render_widget(
            Paragraph::new(format!(
                "PEEK: {person} - READ ONLY\nWorkspace: {person}/{}",
                state.selected_workspace().unwrap_or("unknown")
            ))
            .style(palette.style().fg(palette.subtext0)),
            banner,
        );
        area.y = area.y.saturating_add(banner_height);
        area.height = area.height.saturating_sub(banner_height);
    }
    draw_panes(frame, state, area);
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, status: &str) {
    let palette = Palette::default();
    let spans: Vec<_> = status
        .split("  ")
        .flat_map(|hint| {
            let (key, label) = hint.split_once(' ').unwrap_or((hint, ""));
            [
                Span::styled(
                    format!(" {key}"),
                    palette.style().add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {label} "), palette.style().fg(palette.subtext0)),
            ]
        })
        .collect();
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(palette.style()),
        area,
    );
}

fn draw_panes(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    let Some(tab) = state.visible_tab().cloned() else {
        state.set_pane_areas(Vec::new());
        return;
    };
    let mut input_areas = Vec::new();
    for (pane, pane_area) in pane_rects(&tab, area) {
        let palette = Palette::default();
        let block = palette
            .block(state.focused() == Some(pane.as_str()))
            .title(Span::styled(
                format!(" {pane} "),
                palette.style().fg(palette.subtext0),
            ));
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
        let palette = Palette::default();
        buffer.set_style(area, palette.style());
        for (row_index, row) in self.rows.iter().take(area.height as usize).enumerate() {
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
        .fg(color(cell.fg, palette.text, palette))
        .bg(color(cell.bg, palette.panel_bg, palette))
        .add_modifier(modifiers)
}

fn color(color: Color, default: ratatui::style::Color, palette: Palette) -> ratatui::style::Color {
    match color {
        Color::Default => default,
        Color::Indexed(index) if index < 16 => palette.ansi(index),
        Color::Indexed(index) => ratatui::style::Color::Indexed(index),
        Color::Rgb { red, green, blue } => ratatui::style::Color::Rgb(red, green, blue),
    }
}

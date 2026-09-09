use crate::{
    state::{ClientState, Tile},
    terminal_cells::PaneCells,
    theme::Palette,
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    widgets::Paragraph,
};
use seer_core::proto::TerminalInfo;

const MIN_ROW_HEIGHT: usize = 9;

pub(super) fn draw(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect, columns: usize) {
    let terminals = state.selected_terminals().to_vec();
    if terminals.is_empty() {
        state.grid_columns = columns;
        empty(frame, state, area);
        return;
    }
    state.focus = state.focus.min(terminals.len() - 1);
    if let [terminal] = terminals.as_slice() {
        tile(frame, state, 0, terminal, area, false);
        return;
    }
    let total_rows = terminals.len().div_ceil(columns);
    let capacity = (usize::from(area.height) + 1) / MIN_ROW_HEIGHT;
    let visible_rows = total_rows
        .min(if total_rows > capacity {
            usize::from(area.height) / MIN_ROW_HEIGHT
        } else {
            capacity
        })
        .max(1);
    if (columns, visible_rows) != (state.grid_columns, state.grid_rows) {
        let focus_row = state.focus / columns;
        state.grid_scroll = state
            .grid_scroll
            .clamp(focus_row.saturating_sub(visible_rows - 1), focus_row);
    }
    state.grid_columns = columns;
    state.grid_rows = visible_rows;
    state.grid_scroll = state
        .grid_scroll
        .min(total_rows.saturating_sub(visible_rows));
    let remaining = terminals
        .len()
        .saturating_sub((state.grid_scroll + visible_rows) * columns);
    super::more(frame, area, remaining);
    let area = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(u16::from(remaining > 0)),
    );
    for (index, terminal) in terminals
        .iter()
        .enumerate()
        .skip(state.grid_scroll * columns)
        .take(visible_rows * columns)
    {
        let row = index / columns - state.grid_scroll;
        let column = index % columns;
        let x = area.x + (usize::from(area.width) * column / columns) as u16;
        let right = area.x + (usize::from(area.width) * (column + 1) / columns) as u16;
        let y = area.y + (usize::from(area.height) * row / visible_rows) as u16;
        let bottom = area.y + (usize::from(area.height) * (row + 1) / visible_rows) as u16;
        let rect = Rect::new(
            x,
            y,
            (right - x).saturating_sub(u16::from(column + 1 < columns)),
            bottom - y,
        );
        tile(frame, state, index, terminal, rect, true);
    }
}

fn tile(
    frame: &mut Frame<'_>,
    state: &mut ClientState,
    index: usize,
    terminal: &TerminalInfo,
    rect: Rect,
    bordered: bool,
) {
    let content = if bordered {
        let palette = Palette::default();
        let block = palette
            .block(state.chrome.grid_focus && index == state.focus)
            .title(format!(" {} {} ", index + 1, terminal.name));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        inner
    } else {
        rect
    };
    if content.is_empty() {
        return;
    }
    let rows = state
        .frames
        .get(&(state.user().into(), terminal.pane.clone()))
        .map_or(&[][..], |f| f.rows.as_slice());
    frame.render_widget(PaneCells::new(rows), content);
    state.box_areas.push(Tile {
        index,
        area: rect,
        content,
    });
}

fn empty(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let palette = Palette::default();
    frame.render_widget(
        Paragraph::new(if state.user() == state.own_user {
            "No terminals.\nRight click the control at the top right to open one."
        } else {
            "No terminals."
        })
        .style(palette.style().fg(palette.subtext0))
        .alignment(Alignment::Center),
        Rect::new(
            area.x,
            area.y + area.height.saturating_sub(2) / 2,
            area.width,
            area.height.min(2),
        ),
    );
}

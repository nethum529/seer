use crate::{state::ClientState, theme::Palette};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

const COLUMN_ROWS: usize = 5;
const COLUMN_GAP: u16 = 2;
const FRAME_ROWS: u16 = 6;

struct Picker {
    area: Rect,
    rows: usize,
    column_width: u16,
}

pub(super) fn draw(frame: &mut Frame<'_>, state: &mut ClientState, full: Rect) {
    let palette = Palette::default();
    let user = super::panels::viewed(state).to_owned();
    let granted = state.may_type(&user);
    let permission = format!(
        "Permissions {} for {}",
        if granted { "granted" } else { "not granted" },
        state.display_name(&user)
    );
    let labels: Vec<String> = state
        .people
        .iter()
        .map(|person| {
            format!(
                "{} {}{}",
                if person.user_id == user {
                    "\u{2713}"
                } else {
                    " "
                },
                state.display_name(&person.user_id),
                if person.host { "  host" } else { "" },
            )
        })
        .collect();
    let picker = measure(full, &labels, &permission, &state.server);
    let style = palette.style().bg(palette.surface0);
    palette.clear(frame.buffer_mut(), picker.area);
    let block = palette.block(false).style(style);
    let inner = block.inner(picker.area);
    frame.render_widget(block, picker.area);
    state.chrome.panel_area = picker.area;
    if inner.is_empty() {
        return;
    }
    frame.render_widget(
        Paragraph::new(state.server.clone()).style(style.fg(palette.overlay0)),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
    rule(frame, inner, inner.y + 1);
    rule(frame, inner, inner.bottom().saturating_sub(2));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "\u{25cf} ",
                style.fg(if granted { palette.green } else { palette.red }),
            ),
            Span::styled(permission, style),
        ])),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let height = inner.height.saturating_sub(4);
    let list = Rect::new(inner.x, inner.y + 2, inner.width, height);
    users(frame, state, list, &picker, &labels);
}

fn width(text: &str) -> u16 {
    Span::raw(text).width() as u16
}

fn measure(full: Rect, labels: &[String], permission: &str, server: &str) -> Picker {
    let widest = labels.iter().map(|label| width(label)).max().unwrap_or(0);
    let column_width = (widest + COLUMN_GAP).max(1);
    let rows = usize::from(full.height.saturating_sub(1 + FRAME_ROWS))
        .clamp(1, COLUMN_ROWS)
        .min(labels.len().max(1));
    let columns = usize::from(full.width.saturating_sub(2) / column_width)
        .clamp(1, labels.len().div_ceil(rows).max(1));
    let hidden = labels.len() > rows * columns;
    let width = (column_width * columns as u16)
        .max(width(permission) + 2)
        .max(width(server))
        .saturating_add(2)
        .min(full.width);
    let height = (FRAME_ROWS + rows as u16 + u16::from(hidden)).min(full.height);
    Picker {
        area: Rect::new(
            full.right() - width,
            (full.y + 1).min(full.bottom().saturating_sub(height)),
            width,
            height,
        ),
        rows,
        column_width,
    }
}

fn rule(frame: &mut Frame<'_>, inner: Rect, y: u16) {
    if y <= inner.y || y >= inner.bottom() {
        return;
    }
    let palette = Palette::default();
    frame.render_widget(
        Paragraph::new("-".repeat(usize::from(inner.width)))
            .style(palette.style().bg(palette.surface0).fg(palette.overlay0)),
        Rect::new(inner.x, y, inner.width, 1),
    );
}

fn users(
    frame: &mut Frame<'_>,
    state: &mut ClientState,
    list: Rect,
    picker: &Picker,
    labels: &[String],
) {
    let palette = Palette::default();
    let rows = picker.rows.min(usize::from(list.height));
    if rows == 0 || list.width == 0 {
        return;
    }
    let columns = usize::from(list.width / picker.column_width).max(1);
    let total = labels.len().div_ceil(rows);
    state.picker_scroll = state.picker_scroll.min(total.saturating_sub(columns));
    let first = state.picker_scroll * rows;
    let shown = (rows * columns).min(labels.len().saturating_sub(first));
    for (place, index) in (first..first + shown).enumerate() {
        let x = list.x + (place / rows) as u16 * picker.column_width;
        let rect = Rect::new(
            x,
            list.y + (place % rows) as u16,
            picker.column_width.min(list.right().saturating_sub(x)),
            1,
        );
        let mut style = palette.style().bg(palette.surface0);
        if index == state.selected {
            style = style.bg(palette.surface1).add_modifier(Modifier::BOLD);
        }
        frame.render_widget(Paragraph::new(labels[index].clone()).style(style), rect);
        state.people_areas.push((index, rect));
    }
    cue(frame, list, labels.len() - shown, rows);
}

fn cue(frame: &mut Frame<'_>, list: Rect, hidden: usize, rows: usize) {
    if hidden == 0 || usize::from(list.height) <= rows {
        return;
    }
    let palette = Palette::default();
    frame.render_widget(
        Paragraph::new(format!("+{hidden} more"))
            .style(palette.style().bg(palette.surface0).fg(palette.overlay0)),
        Rect::new(list.x, list.y + rows as u16, list.width, 1),
    );
}

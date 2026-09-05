use crate::{state::ClientState, theme::Palette};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Padding, Paragraph},
};

pub(super) fn footer(frame: &mut Frame<'_>, state: &mut ClientState, hints: &str) {
    let palette = Palette::default();
    let area = frame.area();
    let line = hint_line(hints);
    let width = (line.width() as u16).min(area.width);
    let row = Rect::new(
        area.right().saturating_sub(width),
        area.bottom().saturating_sub(1),
        width,
        area.height.min(1),
    );
    state.chrome.footer_areas = hint_areas(hints, row);
    frame.render_widget(
        Paragraph::new(line)
            .style(palette.style())
            .alignment(Alignment::Right),
        Rect::new(
            area.x,
            area.bottom().saturating_sub(1),
            area.width,
            area.height.min(1),
        ),
    );
}

pub(super) fn dialog(frame: &mut Frame<'_>, state: &mut ClientState, title: &str, text: &str) {
    let palette = Palette::default();
    let area = frame.area();
    let width = 36.min(area.width);
    let height = 7.min(area.height);
    let rect = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    palette.clear(frame.buffer_mut(), rect);
    let mut line = hint_line(text);
    for span in &mut line.spans {
        span.style = span.style.bg(palette.surface0);
    }
    let keys_width = (line.width() as u16).min(rect.width.saturating_sub(4));
    state.chrome.dialog_areas = hint_areas(
        text,
        Rect::new(
            rect.x + rect.width.saturating_sub(keys_width) / 2,
            rect.y + 3,
            keys_width,
            1,
        ),
    );
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), line])
            .style(palette.style().bg(palette.surface0))
            .alignment(Alignment::Center)
            .block(
                palette
                    .block(false)
                    .style(palette.style().bg(palette.surface0))
                    .padding(Padding::uniform(1))
                    .title(format!(" {title} ")),
            ),
        rect,
    );
}

pub(super) fn notice(frame: &mut Frame<'_>, state: &ClientState, hints: &str) {
    let area = frame.area();
    frame.render_widget(
        Paragraph::new(state.notice.as_str()).style(Palette::default().style()),
        Rect::new(
            area.x + 1,
            area.bottom().saturating_sub(1),
            area.width.saturating_sub(hints.len() as u16 + 3),
            area.height.min(1),
        ),
    );
}

fn hint_line(hints: &str) -> Line<'static> {
    let palette = Palette::default();
    Line::from(
        hints
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
            .collect::<Vec<_>>(),
    )
}

fn hint_areas(hints: &str, area: Rect) -> Vec<(String, Rect)> {
    let mut x = area.x;
    hints
        .split("  ")
        .map(|hint| {
            let width = (hint.len() as u16 + 2).min(area.right().saturating_sub(x));
            let item = (
                hint.split_once(' ').map_or(hint, |(key, _)| key).into(),
                Rect::new(x, area.y, width, area.height),
            );
            x += width;
            item
        })
        .collect()
}

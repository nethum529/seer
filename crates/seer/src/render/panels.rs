use crate::{
    panels::{Panel, Row, rows},
    state::ClientState,
    theme::Palette,
};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
    text::{Line, Span},
    widgets::Paragraph,
};

const HANDLE_ROWS: u16 = 3;
const PINNED_WIDTH: u16 = 20;
const PINNED_MIN_WIDTH: u16 = 60;

pub(super) fn reserve(state: &mut ClientState, full: Rect) -> Rect {
    state.chrome.pinned_area = Rect::default();
    if !state.chrome.pinned || full.is_empty() || full.width < PINNED_MIN_WIDTH {
        return full;
    }
    state.chrome.pinned_area = Rect::new(full.x, full.y, PINNED_WIDTH, full.height);
    Rect::new(
        full.x + PINNED_WIDTH,
        full.y,
        full.width - PINNED_WIDTH,
        full.height,
    )
}

pub(super) fn draw(frame: &mut Frame<'_>, state: &mut ClientState, content: Rect) {
    let full = frame.area();
    state.chrome.handle_area = Rect::default();
    state.chrome.chip_area = Rect::default();
    state.chrome.panel_area = Rect::default();
    state.chrome.close_area = Rect::default();
    state.chrome.search_area = Rect::default();
    state.chrome.pin_area = Rect::default();
    state.chrome.rows.clear();
    if full.is_empty() {
        return;
    }
    let pinned = state.chrome.pinned_area;
    if !pinned.is_empty() {
        column(frame, state, pinned, true);
        if state.chrome.panel == Some(Panel::People) {
            state.chrome.panel_area = pinned;
        }
    } else if state.chrome.panel != Some(Panel::People) {
        handle(frame, state, full);
    }
    chip(frame, state, full);
    match state.chrome.panel {
        Some(Panel::People) if pinned.is_empty() => people(frame, state, full),
        Some(Panel::Session) => session(frame, state, content),
        _ => {}
    }
}

fn handle(frame: &mut Frame<'_>, state: &mut ClientState, full: Rect) {
    if full.height < HANDLE_ROWS {
        return;
    }
    let palette = Palette::default();
    let area = Rect::new(
        full.x,
        full.y + (full.height - HANDLE_ROWS) / 2,
        1,
        HANDLE_ROWS,
    );
    frame.render_widget(
        Paragraph::new(vec![Line::from("\u{00b7}"); usize::from(HANDLE_ROWS)])
            .style(palette.style().fg(palette.overlay0).bg(palette.surface0)),
        area,
    );
    state.chrome.handle_area = area;
}

fn access(state: &ClientState) -> (&'static str, bool) {
    let user = state
        .viewer
        .as_ref()
        .map_or(state.user(), |viewer| viewer.user.as_str());
    if user == state.own_user {
        ("Your terminal", false)
    } else if state.may_type(user) {
        ("Can type", true)
    } else {
        ("Read only", true)
    }
}

fn chip(frame: &mut Frame<'_>, state: &mut ClientState, full: Rect) {
    let palette = Palette::default();
    let (label, remote) = access(state);
    let style = palette.style().bg(palette.surface0);
    let mark = Span::styled(" seer ", style.fg(palette.overlay0));
    let access = Span::styled(
        format!(" {label} "),
        style.fg(if remote && label == "Read only" {
            palette.subtext0
        } else {
            palette.accent
        }),
    );
    let spans = if (mark.width() + access.width()) as u16 <= full.width {
        vec![mark, access]
    } else if remote {
        vec![Span::styled(label, access.style)]
    } else {
        vec![mark]
    };
    let line = Line::from(spans);
    let width = (line.width() as u16).min(full.width);
    let area = Rect::new(full.right() - width, full.y, width, 1);
    frame.render_widget(Paragraph::new(line).style(style), area);
    state.chrome.chip_area = area;
}

fn people(frame: &mut Frame<'_>, state: &mut ClientState, full: Rect) {
    let width = if full.width < PINNED_MIN_WIDTH {
        16
    } else {
        PINNED_WIDTH
    }
    .min(full.width);
    let area = Rect::new(full.x, full.y, width, full.height);
    column(frame, state, area, false);
    state.chrome.panel_area = area;
}

fn column(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect, pinned: bool) {
    let palette = Palette::default();
    palette.clear(frame.buffer_mut(), area);
    super::people::column(frame, state, area, pinned);
    solidify(frame.buffer_mut(), area, palette.panel_bg);
}

fn solidify(buffer: &mut Buffer, area: Rect, background: Color) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if buffer[(x, y)].bg == Color::Reset {
                buffer[(x, y)].bg = background;
            }
        }
    }
}

fn label(state: &ClientState, row: Row) -> String {
    match row {
        Row::Terminal(index) => state
            .selected_terminals()
            .get(index)
            .map_or_else(String::new, |terminal| {
                format!("{} {}", index + 1, terminal.name)
            }),
        Row::New => "n new terminal".into(),
        Row::Close => "x close terminal".into(),
        Row::Back => "back".into(),
        Row::CopyInvite => "c copy invite".into(),
        Row::Quit => "q quit".into(),
    }
}

fn header(state: &ClientState) -> Vec<Line<'static>> {
    let palette = Palette::default();
    let dim = palette.style().bg(palette.surface0).fg(palette.subtext0);
    let user = state
        .viewer
        .as_ref()
        .map_or(state.user(), |viewer| viewer.user.as_str());
    let name = if user == state.own_user {
        "you".to_owned()
    } else {
        state.person_name(user).to_owned()
    };
    let (access, _) = access(state);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("person ", dim),
            Span::styled(
                name,
                palette
                    .style()
                    .bg(palette.surface0)
                    .fg(palette.text)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("access ", dim),
            Span::styled(access, palette.style().bg(palette.surface0)),
        ]),
    ];
    if !state.server.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("host   ", dim),
            Span::styled(state.server.clone(), palette.style().bg(palette.surface0)),
        ]));
    }
    lines.extend(super::people::status_lines(state).into_iter().map(|line| {
        Line::from(
            line.spans
                .into_iter()
                .map(|span| Span::styled(span.content, span.style.bg(palette.surface0)))
                .collect::<Vec<_>>(),
        )
    }));
    lines
}

fn session(frame: &mut Frame<'_>, state: &mut ClientState, full: Rect) {
    let palette = Palette::default();
    let mut head = header(state);
    head.truncate(usize::from(full.height.saturating_sub(5)));
    let actions = rows(state);
    let labels: Vec<String> = actions.iter().map(|row| label(state, *row)).collect();
    let widest = head
        .iter()
        .map(Line::width)
        .chain(labels.iter().map(|label| label.chars().count()))
        .max()
        .unwrap_or(0);
    let width = ((widest + 4).clamp(28, 48) as u16).min(full.width);
    let height = ((head.len() + labels.len() + 3) as u16).min(full.height.saturating_sub(1).max(1));
    let area = Rect::new(
        full.right() - width,
        (full.y + 1).min(full.bottom().saturating_sub(height)),
        width,
        height,
    );
    palette.clear(frame.buffer_mut(), area);
    let block = palette
        .block(false)
        .title(" session ")
        .style(palette.style().bg(palette.surface0));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    state.chrome.panel_area = area;
    state.chrome.close_area =
        Rect::new(area.right().saturating_sub(3), area.y, area.width.min(3), 1);
    frame.render_widget(
        Paragraph::new(" x ").style(palette.style().bg(palette.surface0)),
        state.chrome.close_area,
    );
    let lines = head.len() as u16 + 1;
    frame.render_widget(
        Paragraph::new(head).style(palette.style().bg(palette.surface0)),
        inner,
    );
    if inner.height <= lines {
        return;
    }
    frame.render_widget(
        Paragraph::new("-".repeat(usize::from(inner.width)))
            .style(palette.style().bg(palette.surface0).fg(palette.overlay0)),
        Rect::new(inner.x, inner.y + lines - 1, inner.width, 1),
    );
    actions_list(
        frame,
        state,
        Rect::new(inner.x, inner.y + lines, inner.width, inner.height - lines),
        &labels,
    );
}

fn actions_list(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect, labels: &[String]) {
    let palette = Palette::default();
    state.chrome.row = state.chrome.row.min(labels.len().saturating_sub(1));
    let capacity = usize::from(area.height);
    let start = state.chrome.row.saturating_sub(capacity.saturating_sub(1));
    for (row, label) in labels.iter().enumerate().skip(start).take(capacity) {
        let style = palette
            .style()
            .bg(palette.surface0)
            .fg(if row == state.chrome.row {
                palette.accent
            } else {
                palette.text
            });
        let rect = Rect::new(area.x, area.y + (row - start) as u16, area.width, 1);
        frame.render_widget(Paragraph::new(label.clone()).style(style), rect);
        state.chrome.rows.push((row, rect));
    }
}

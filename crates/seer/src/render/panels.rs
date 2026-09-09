use crate::{
    panels::{Panel, Row, rows},
    state::ClientState,
    theme::Palette,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

pub(super) fn draw(frame: &mut Frame<'_>, state: &mut ClientState) {
    let full = frame.area();
    state.chrome.chip_area = Rect::default();
    state.chrome.panel_area = Rect::default();
    state.chrome.close_area = Rect::default();
    state.chrome.rows.clear();
    if full.is_empty() {
        return;
    }
    chip(frame, state, full);
    match state.chrome.panel {
        Some(Panel::Picker) => super::picker::draw(frame, state, full),
        Some(Panel::Session) => session(frame, state, full),
        None => {}
    }
}

pub(super) fn viewed(state: &ClientState) -> &str {
    state
        .viewer
        .as_ref()
        .map_or(state.user(), |viewer| viewer.user.as_str())
}

fn access(state: &ClientState) -> &'static str {
    let user = viewed(state);
    if user == state.own_user {
        "Your terminal"
    } else if state.may_type(user) {
        "Can type"
    } else {
        "Read only"
    }
}

fn chip(frame: &mut Frame<'_>, state: &mut ClientState, full: Rect) {
    let palette = Palette::default();
    let style = palette.style().bg(palette.surface0);
    let mark = Span::styled(" Seer ", style.fg(palette.overlay0));
    let name = Span::styled(
        format!("{} ", state.display_name(viewed(state))),
        style.fg(palette.accent),
    );
    let spans = if (mark.width() + name.width()) as u16 <= full.width {
        vec![mark, name]
    } else {
        vec![name]
    };
    let line = Line::from(spans);
    let width = (line.width() as u16).min(full.width);
    let area = Rect::new(full.right() - width, full.y, width, 1);
    frame.render_widget(Paragraph::new(line).style(style), area);
    state.chrome.chip_area = area;
}

pub(super) fn status_lines(state: &ClientState) -> Vec<Line<'static>> {
    let palette = Palette::default();
    let dim = palette.style().fg(palette.subtext0);
    let user = viewed(state);
    let mut lines = Vec::new();
    if let Some(person) = state.people.iter().find(|p| p.user_id == user && !p.online) {
        lines.push(Line::styled(
            format!(" away {}", super::idle_text(person.idle_secs)),
            dim,
        ));
    }
    let focused = state.focused();
    let typist = state
        .terminals
        .get(user)
        .into_iter()
        .flatten()
        .find(|t| Some(t.pane.as_str()) == focused)
        .and_then(|t| t.last_typist.as_deref());
    if let Some(name) = typist {
        lines.push(Line::styled(
            format!(" typing {name}"),
            palette.style().fg(palette.yellow),
        ));
    }
    lines
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
    let mut lines = vec![
        Line::from(vec![
            Span::styled("person ", dim),
            Span::styled(
                state.display_name(viewed(state)).to_owned(),
                palette
                    .style()
                    .bg(palette.surface0)
                    .fg(palette.text)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("access ", dim),
            Span::styled(access(state), palette.style().bg(palette.surface0)),
        ]),
    ];
    if !state.server.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("host   ", dim),
            Span::styled(state.server.clone(), palette.style().bg(palette.surface0)),
        ]));
    }
    lines.extend(status_lines(state).into_iter().map(|line| {
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

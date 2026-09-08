use crate::{state::ClientState, theme::Palette};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

const MAX_TAB_NAME: usize = 16;

pub(super) fn first_run(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let palette = Palette::default();
    let invite = state.invite.as_deref().unwrap_or("Creating an invite...");
    let text = vec![
        Line::styled(
            "Nobody else is here yet.",
            palette
                .style()
                .fg(palette.text)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(
            "Send this line to a friend. Your friend pastes it in a terminal. It expires in 24 hours.",
            palette.style().fg(palette.subtext0),
        ),
        Line::from(""),
        Line::styled(
            invite.rsplit_once(' ').map_or(invite, |(line, _)| line),
            palette.style().fg(palette.blue),
        ),
        Line::styled(
            invite.rsplit_once(' ').map_or("", |(_, capsule)| capsule),
            palette.style().fg(palette.blue),
        ),
        Line::from(""),
        Line::styled(
            "c copy   n new terminal",
            palette.style().fg(palette.subtext0),
        ),
    ];
    let width = area.width.saturating_sub(4).clamp(1, 80);
    let height = 11_u16
        .saturating_add((invite.len() as u16).saturating_div(width))
        .min(area.height);
    let centered = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width.min(area.width),
        height,
    );
    frame.render_widget(
        Paragraph::new(text)
            .style(palette.style())
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        centered,
    );
}

pub(super) fn context_row(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let palette = Palette::default();
    let user = state
        .viewer
        .as_ref()
        .map_or(state.user(), |v| v.user.as_str());
    let person = state.people.iter().find(|p| p.user_id == user);
    let dim = palette.style().fg(palette.subtext0);
    let mut spans = vec![Span::styled(
        state.person_name(user).to_owned(),
        palette
            .style()
            .fg(palette.text)
            .add_modifier(Modifier::BOLD),
    )];
    match person {
        Some(p) if p.online => {
            spans.push(Span::styled("  online", palette.style().fg(palette.green)))
        }
        Some(p) => spans.push(Span::styled(
            format!("  away {}", super::idle_text(p.idle_secs)),
            dim,
        )),
        None => {}
    }
    let allowed = state.may_type(user);
    spans.push(Span::styled(
        if allowed {
            "  input: allowed"
        } else {
            "  input: read only"
        },
        if allowed {
            palette.style().fg(palette.green)
        } else {
            dim
        },
    ));
    let focused = state.focused();
    let typist = state
        .terminals
        .get(user)
        .into_iter()
        .flatten()
        .find(|t| Some(t.pane.as_str()) == focused)
        .and_then(|t| t.last_typist.as_deref());
    if let Some(name) = typist {
        spans.push(Span::styled(
            format!("  {name} is typing"),
            palette.style().fg(palette.yellow),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(palette.style()),
        area,
    );
}

fn tab_label(index: usize, name: &str, busy: bool, closable: bool) -> String {
    let name: String = name.chars().take(MAX_TAB_NAME).collect();
    let mut label = format!(" {} {name}", index + 1);
    if busy {
        label.push('*');
    }
    if closable {
        label.push_str(" x");
    }
    label.push(' ');
    label
}

pub(super) fn tabs(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    let palette = Palette::default();
    let terminals = state.selected_terminals().to_vec();
    let closable = state.user() == state.own_user;
    let labels: Vec<String> = terminals
        .iter()
        .enumerate()
        .map(|(i, t)| tab_label(i, &t.name, t.state != "idle", closable))
        .collect();
    let widths: Vec<u16> = labels.iter().map(|l| l.chars().count() as u16).collect();
    let plus_width = 3_u16.min(area.width);
    let room = u32::from(area.width.saturating_sub(plus_width));
    let focus = state.focus.min(widths.len().saturating_sub(1));
    let mut start = 0;
    while start < focus
        && widths[start..=focus]
            .iter()
            .map(|w| u32::from(*w))
            .sum::<u32>()
            > room
    {
        start += 1;
    }
    let mut x = area.x;
    for (index, label) in labels.iter().enumerate().skip(start) {
        let active = index == state.focus;
        let left = area
            .width
            .saturating_sub(plus_width)
            .saturating_sub(x - area.x);
        let width = widths[index];
        if width > left && !(active && left > 0) {
            break;
        }
        let width = width.min(left);
        let label = cut_label(label, width, closable);
        let rect = Rect::new(x, area.y, width, 1);
        let style = if active {
            palette.style().bg(palette.surface0).fg(palette.accent)
        } else {
            palette.style().fg(palette.subtext0)
        };
        frame.render_widget(Paragraph::new(label).style(style), rect);
        state.tab_areas.push((index, rect));
        x += width;
        if left == 0 {
            break;
        }
    }
    state.plus_area = Rect::new(x, area.y, plus_width.min(area.right().saturating_sub(x)), 1);
    frame.render_widget(
        Paragraph::new(" + ").style(palette.style().fg(palette.accent)),
        state.plus_area,
    );
}

fn cut_label(label: &str, width: u16, closable: bool) -> String {
    let tail = if closable { " x " } else { " " };
    let width = usize::from(width);
    if label.chars().count() <= width {
        return label.into();
    }
    if width <= tail.len() {
        return label.chars().take(width).collect();
    }
    let mut cut: String = label.chars().take(width - tail.len()).collect();
    cut.push_str(tail);
    cut
}

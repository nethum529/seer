use crate::{state::ClientState, theme::Palette};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Modifier,
    text::Line,
    widgets::{Paragraph, Wrap},
};

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

pub(super) fn tabs(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    let palette = Palette::default();
    let terminals = state.selected_terminals().to_vec();
    let mut x = area.x;
    let widths: Vec<_> = terminals
        .iter()
        .map(|t| (t.name.chars().count() + t.state.len() + 5) as u16)
        .collect();
    let mut start = 0;
    while start < state.focus
        && widths[start..=state.focus.min(widths.len().saturating_sub(1))]
            .iter()
            .map(|w| u32::from(*w))
            .sum::<u32>()
            > u32::from(area.width.saturating_sub(3))
    {
        start += 1;
    }
    for (index, terminal) in terminals.iter().enumerate().skip(start) {
        let width = widths[index].min(area.right().saturating_sub(x + 3));
        if width == 0 {
            break;
        }
        let rect = Rect::new(x, area.y, width, area.height.min(1));
        let style = if index == state.focus {
            palette.style().bg(palette.surface0).fg(palette.accent)
        } else {
            palette.style().fg(palette.subtext0)
        };
        frame.render_widget(
            Paragraph::new(format!(" {} {} x ", terminal.name, terminal.state)).style(style),
            rect,
        );
        state.tab_areas.push((index, rect));
        x += width;
    }
    state.plus_area = Rect::new(
        x,
        area.y,
        area.right().saturating_sub(x).min(3),
        area.height.min(1),
    );
    frame.render_widget(
        Paragraph::new(" + ").style(palette.style().fg(palette.accent)),
        state.plus_area,
    );
}

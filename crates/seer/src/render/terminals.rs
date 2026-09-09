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
            "Click seer at the top right to copy the invite or open a terminal.",
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

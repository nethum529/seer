use crate::{state::ClientState, theme::Palette};
use ratatui::{Frame, layout::Rect, widgets::Paragraph};

pub(super) fn notice(frame: &mut Frame<'_>, state: &ClientState) {
    let area = frame.area();
    let notice = match (&state.room_notice, &state.standing_notice) {
        (Some(room), _) if state.notice.is_empty() => room,
        (None, Some(standing)) if state.notice.is_empty() => standing,
        _ => &state.notice,
    };
    if notice.is_empty() || area.is_empty() {
        return;
    }
    let palette = Palette::default();
    let text = format!(" {notice} ");
    let width = (text.chars().count() as u16).min(area.width);
    frame.render_widget(
        Paragraph::new(text).style(palette.style().bg(palette.surface0)),
        Rect::new(area.x, area.bottom() - 1, width, 1),
    );
}

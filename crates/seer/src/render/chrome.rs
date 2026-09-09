use crate::{state::ClientState, theme::Palette};
use ratatui::{Frame, layout::Rect, widgets::Paragraph};

pub(super) fn notice(frame: &mut Frame<'_>, state: &ClientState) {
    let area = frame.area();
    if state.notice.is_empty() || area.is_empty() {
        return;
    }
    let palette = Palette::default();
    let text = format!(" {} ", state.notice);
    let width = (text.chars().count() as u16).min(area.width);
    frame.render_widget(
        Paragraph::new(text).style(palette.style().bg(palette.surface0)),
        Rect::new(area.x, area.bottom() - 1, width, 1),
    );
}

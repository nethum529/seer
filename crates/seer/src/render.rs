mod chrome;
mod grid;
mod panels;
mod people;
mod terminals;
use crate::{state::ClientState, theme::Palette};
use ratatui::{Frame, layout::Rect, widgets::Paragraph};

#[derive(Default)]
pub(crate) struct Chrome {
    pub(crate) grid_focus: bool,
    pub(crate) panel: Option<crate::panels::Panel>,
    pub(crate) row: usize,
    pub(crate) handle_area: Rect,
    pub(crate) chip_area: Rect,
    pub(crate) panel_area: Rect,
    pub(crate) close_area: Rect,
    pub(crate) search_area: Rect,
    pub(crate) pinned: bool,
    pub(crate) pin_area: Rect,
    pub(crate) pinned_area: Rect,
    pub(crate) rows: Vec<(usize, Rect)>,
    pub(crate) context: Option<crate::person_menu::TerminalMenu>,
    pub(crate) dialog_areas: Vec<(String, Rect)>,
    notice: String,
    pub(crate) notice_since: Option<std::time::Instant>,
}

pub(crate) fn expire_notice(state: &mut ClientState) -> bool {
    if state.chrome.notice != state.notice || state.chrome.notice_since.is_none() {
        state.chrome.notice.clone_from(&state.notice);
        state.chrome.notice_since = Some(std::time::Instant::now());
    }
    if !state.notice.is_empty()
        && state
            .chrome
            .notice_since
            .is_some_and(|time| time.elapsed().as_secs() >= 3)
    {
        state.notice.clear();
        state.chrome.notice.clear();
        return true;
    }
    false
}

pub(crate) fn draw(frame: &mut Frame<'_>, state: &mut ClientState) {
    let palette = Palette::default();
    let full = frame.area();
    frame.render_widget(Paragraph::new("").style(palette.style()), full);
    state.people_areas.clear();
    state.box_areas.clear();
    let content = panels::reserve(state, full);
    if state.viewer.is_some() {
        crate::viewer::draw(frame, state, content);
    } else if state.people.len() == 1 && state.selected_terminals().is_empty() {
        terminals::first_run(frame, state, content);
    } else {
        grid::draw(
            frame,
            state,
            content,
            if content.width >= 80 { 2 } else { 1 },
        );
    }
    if let Some(selection) = &state.selection {
        selection.draw(frame.buffer_mut());
    }
    panels::draw(frame, state, content);
    chrome::notice(frame, state);
    crate::person_menu::draw(frame, state);
    crate::person_menu::draw_context(frame, state);
    state.chrome.dialog_areas.clear();
    if state.quit_prompt {
        chrome::dialog(frame, state, "Quit seer?", "enter quit  esc stay");
    }
}

pub(crate) fn idle_text(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3600)
    }
}

fn more(frame: &mut Frame<'_>, area: Rect, remaining: usize) {
    if remaining > 0 && !area.is_empty() {
        frame.render_widget(
            Paragraph::new(format!("+{remaining} more"))
                .style(Palette::default().style().fg(Palette::default().overlay0)),
            Rect::new(area.x, area.bottom() - 1, area.width, 1),
        );
    }
}

mod chrome;
use chrome::{dialog, footer, notice};
mod grid;
mod people;
mod terminals;
use crate::{state::ClientState, theme::Palette};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Modifier,
    text::Line,
    widgets::Paragraph,
};
use terminals::{context_spans, first_run, tabs};

#[derive(Default)]
pub(crate) struct Chrome {
    pub(crate) grid_focus: bool,
    pub(crate) show_people: Option<bool>,
    pub(crate) people_open: bool,
    pub(crate) people_toggle_area: Rect,
    pub(crate) context: Option<crate::person_menu::TerminalMenu>,
    pub(crate) footer_areas: Vec<(String, Rect)>,
    pub(crate) dialog_areas: Vec<(String, Rect)>,
    pub(crate) people_area: Rect,
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

fn body_areas(full: Rect, open: bool) -> (Rect, Rect) {
    let body = Rect::new(full.x, full.y, full.width, full.height.saturating_sub(1));
    let width = if !open {
        0
    } else if full.width < 90 {
        14
    } else {
        20
    }
    .min(body.width);
    let people = Rect::new(body.x, body.y, width, body.height);
    (
        people,
        Rect::new(
            people.right(),
            body.y,
            body.width.saturating_sub(width),
            body.height,
        ),
    )
}

pub(crate) fn draw(frame: &mut Frame<'_>, state: &mut ClientState) {
    let palette = Palette::default();
    let full = frame.area();
    state.chrome.people_open = state.chrome.show_people.unwrap_or(full.width >= 50);
    state.chrome.people_toggle_area = Rect::default();
    frame.render_widget(Paragraph::new("").style(palette.style()), full);
    state.people_areas.clear();
    state.box_areas.clear();
    state.tab_areas.clear();
    state.plus_area = Rect::default();
    let (people_area, terminals) = body_areas(full, state.chrome.people_open);
    state.chrome.people_area = people_area;
    people::column(frame, state, people_area);
    terminal_area(frame, state, terminals);
    if let Some(selection) = &state.selection {
        selection.draw(frame.buffer_mut());
    }
    let hints = hints(state, full.width);
    footer(frame, state, &hints);
    notice(frame, state, &hints);
    crate::person_menu::draw(frame, state);
    crate::person_menu::draw_context(frame, state);
    state.chrome.dialog_areas.clear();
    if state.quit_prompt {
        dialog(frame, state, "Quit seer?", "enter quit  esc stay");
    }
}

fn hints(state: &ClientState, width: u16) -> String {
    if state.viewer.is_some() {
        return "ctrl+b back".into();
    }
    if state.searching {
        return "enter select  esc cancel".into();
    }
    let toggle = if state.chrome.people_open {
        "p hide"
    } else {
        "p people"
    };
    let base = if width < 50 {
        format!("{toggle}  enter view  n new  q quit")
    } else if width < 90 {
        format!("j/k people  {toggle}  enter view  n new  x close  q quit")
    } else {
        format!(
            "j/k people  {toggle}  h/l boxes  enter view  n new  x close  1-9 tabs  / find  q quit"
        )
    };
    if state.people.len() != 1 || state.invite.is_none() {
        return base;
    }
    let fits = |hints: &str| hints.chars().count() + 2 <= usize::from(width);
    let with_copy = |base: &str, copy: &str| base.replace("n new", &format!("{copy}  n new"));
    let trimmed = base.replace("x close  ", "");
    [
        with_copy(&base, "c copy invite"),
        with_copy(&base, "c copy"),
        with_copy(&trimmed, "c copy"),
    ]
    .into_iter()
    .find(|hints| fits(hints))
    .unwrap_or_else(|| with_copy(&trimmed.replacen(&format!("{toggle}  "), "", 1), "c copy"))
}

fn meta(state: &ClientState, room: u16) -> Option<String> {
    let online = state.people.iter().filter(|person| person.online).count();
    let full = format!("{}  {online} online ", state.server);
    if full.chars().count() as u16 <= room {
        return Some(full);
    }
    let short = format!("{online} online ");
    (short.chars().count() as u16 <= room).then_some(short)
}

fn strip(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    let palette = Palette::default();
    let mut rest = area;
    if state.chrome.people_area.width < 2 {
        let label = " > people ";
        let toggle = Rect::new(area.x, area.y, (label.len() as u16).min(area.width), 1);
        state.chrome.people_toggle_area = toggle;
        frame.render_widget(
            Paragraph::new(label).style(
                palette
                    .style()
                    .fg(palette.overlay0)
                    .add_modifier(Modifier::BOLD),
            ),
            toggle,
        );
        rest = Rect::new(
            toggle.right(),
            area.y,
            area.width.saturating_sub(toggle.width),
            1,
        );
    }
    if rest.is_empty() {
        return;
    }
    let line = Line::from(context_spans(state, rest.width));
    let used = line.width() as u16;
    frame.render_widget(Paragraph::new(line).style(palette.style()), rest);
    let Some(text) = meta(state, rest.width.saturating_sub(used.saturating_add(2))) else {
        return;
    };
    let width = (text.chars().count() as u16).min(rest.width);
    frame.render_widget(
        Paragraph::new(text)
            .style(palette.style().fg(palette.subtext0))
            .alignment(Alignment::Right),
        Rect::new(rest.right().saturating_sub(width), area.y, width, 1),
    );
}

fn terminal_area(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    if area.is_empty() {
        return;
    }
    let mut content = area;
    if people::compact(state.chrome.people_area) && area.height > 1 {
        strip(frame, state, Rect::new(area.x, area.y, area.width, 1));
        content.y += 1;
        content.height -= 1;
    }
    let alone = state.people.len() == 1;
    if state.viewer.is_none() && alone && state.selected_terminals().is_empty() {
        first_run(frame, state, content);
        return;
    }
    if content.height > 1 {
        tabs(
            frame,
            state,
            Rect::new(content.x, content.y, content.width, 1),
        );
        content.y += 1;
        content.height -= 1;
    }
    if state.viewer.is_some() {
        crate::viewer::draw(frame, state, content);
    } else {
        grid::draw(frame, state, content, if area.width >= 80 { 2 } else { 1 });
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

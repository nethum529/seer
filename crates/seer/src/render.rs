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
    text::{Line, Span},
    widgets::Paragraph,
};
use terminals::{context_row, first_run, tabs};

#[derive(Default)]
pub(crate) struct Chrome {
    pub(crate) grid_focus: bool,
    pub(crate) show_people: bool,
    pub(crate) narrow: bool,
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

fn body_areas(full: Rect, show_people: bool) -> (Rect, Rect) {
    let body = Rect::new(
        full.x,
        full.y.saturating_add(1),
        full.width,
        full.height.saturating_sub(2),
    );
    let width = if full.width < 50 && !show_people {
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
    state.chrome.narrow = full.width < 50;
    frame.render_widget(Paragraph::new("").style(palette.style()), full);
    state.people_areas.clear();
    state.box_areas.clear();
    state.tab_areas.clear();
    state.plus_area = Rect::default();
    top_bar(frame, state);
    let (people_area, terminals) = body_areas(full, state.chrome.show_people);
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
    let base = if width < 50 {
        "p people  enter view  n new  q quit"
    } else if width < 90 {
        "j/k people  enter view  n new  x close  q quit"
    } else {
        "j/k people  h/l boxes  enter view  n new  x close  1-9 tabs  / find  q quit"
    };
    if state.people.len() != 1 || state.invite.is_none() {
        return base.into();
    }
    let fits = |hints: &str| hints.chars().count() + 2 <= usize::from(width);
    let with_copy = |base: &str, copy: &str| base.replace("n new", &format!("{copy}  n new"));
    let hints = with_copy(base, "c copy invite");
    if fits(&hints) {
        return hints;
    }
    let hints = with_copy(base, "c copy");
    if fits(&hints) {
        return hints;
    }
    with_copy(&base.replace("x close  ", ""), "c copy")
}

fn top_bar(frame: &mut Frame<'_>, state: &ClientState) {
    let palette = Palette::default();
    let area = frame.area();
    let left = Line::from(vec![
        Span::styled(
            " seer ",
            palette
                .style()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}  {}", state.own_name, state.server),
            palette.style().fg(palette.subtext0),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(left).style(palette.style()),
        Rect::new(area.x, area.y, area.width, area.height.min(1)),
    );
    let right = format!(
        "{} online ",
        state.people.iter().filter(|person| person.online).count()
    );
    let width = (right.len() as u16).min(area.width / 2);
    frame.render_widget(
        Paragraph::new(right)
            .style(palette.style().fg(palette.subtext0))
            .alignment(Alignment::Right),
        Rect::new(
            area.right().saturating_sub(width),
            area.y,
            width,
            area.height.min(1),
        ),
    );
}

fn terminal_area(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    let area = Rect::new(
        area.x.saturating_add(1),
        area.y,
        area.width.saturating_sub(1),
        area.height,
    );
    if area.is_empty() {
        return;
    }
    let alone = state.people.len() == 1;
    if state.viewer.is_none() && alone && state.selected_terminals().is_empty() {
        first_run(frame, state, area);
        return;
    }
    context_row(frame, state, Rect::new(area.x, area.y, area.width, 1));
    let mut content = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );
    if area.height > 2 {
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

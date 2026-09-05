mod chrome;
use chrome::{dialog, footer, notice};
mod terminals;
use crate::terminal_cells::PaneCells;
use crate::{state::ClientState, theme::Palette};
use ratatui::{
    Frame,
    layout::{Alignment, Margin, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Padding, Paragraph},
};
use terminals::{first_run, tabs};

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

pub(crate) fn body_areas(full: Rect, show_people: bool) -> (Rect, Rect) {
    let body = Rect::new(
        full.x.saturating_add(1),
        full.y.saturating_add(1),
        full.width.saturating_sub(2),
        full.height.saturating_sub(2),
    );
    let width = if full.width < 50 && !show_people {
        0
    } else if full.width < 90 {
        14
    } else {
        26
    }
    .min(body.width);
    let people = Rect::new(body.x, body.y, width, body.height);
    let gap = u16::from(width > 0).min(body.width.saturating_sub(width));
    (
        people,
        Rect::new(
            people.right() + gap,
            body.y,
            body.width.saturating_sub(width + gap),
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
    let (people, terminals) = body_areas(full, state.chrome.show_people);
    state.chrome.people_area = people;
    people_column(frame, state, people);
    terminal_area(frame, state, terminals);
    if let Some(selection) = &state.selection {
        selection.draw(frame.buffer_mut());
    }
    let hints = if state.viewer.is_some() {
        "esc back  tab next terminal  q quit"
    } else if state.searching {
        "enter select  esc cancel"
    } else {
        "j/k people  h/l boxes  enter view  n new  x close  1-9 tabs  / find  esc back  q quit"
    };
    let hints: String = if full.width < 50 {
        if state.viewer.is_some() {
            "esc back  tab next  p people  q quit"
        } else {
            "p people  enter view  n new  q quit"
        }
        .into()
    } else if full.width < 90 && state.viewer.is_none() && !state.searching {
        "j/k people  enter view  n new  x close  q quit".into()
    } else {
        hints.into()
    };
    let hints = if state.people.len() == 1 && state.viewer.is_none() && state.invite.is_some() {
        hints.replace("n new", "c copy  n new")
    } else {
        hints
    };
    footer(frame, state, &hints);
    notice(frame, state, &hints);
    crate::person_menu::draw(frame, state);
    crate::person_menu::draw_context(frame, state);
    state.chrome.dialog_areas.clear();
    if state.quit_prompt {
        dialog(frame, state, "Quit seer?", "enter quit  esc stay");
    }
}

pub(crate) fn top_bar(frame: &mut Frame<'_>, state: &ClientState) {
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

pub(crate) fn people_column(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    let palette = Palette::default();
    let title = if state.search.is_empty() && !state.searching {
        " people ".into()
    } else {
        format!(" people /{} ", state.search)
    };
    let block = palette
        .block(!state.chrome.grid_focus)
        .title(title)
        .padding(Padding::new(1, 1, 1, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let matches = state.matches();
    let visible = usize::from(inner.height)
        .saturating_sub(usize::from(matches.len() > usize::from(inner.height)));
    state.people_scroll = state
        .people_scroll
        .min(matches.len().saturating_sub(visible));
    let remaining = matches.len().saturating_sub(state.people_scroll + visible);
    more(frame, inner, remaining);
    for (row, index) in matches
        .into_iter()
        .skip(state.people_scroll)
        .take(visible)
        .enumerate()
    {
        let person = &state.people[index];
        let own = person.user_id == state.own_user;
        let label = if own { "you" } else { &person.name };
        let marker = if state
            .viewer
            .as_ref()
            .is_some_and(|viewer| viewer.user == person.user_id)
        {
            ">"
        } else {
            " "
        };
        let mut style = palette
            .style()
            .fg(if own { palette.yellow } else { palette.text });
        if index == state.selected {
            style = style.bg(palette.surface0);
        }
        let rect = Rect::new(
            area.x.saturating_add(1),
            inner.y + row as u16,
            area.width.saturating_sub(2),
            1,
        );
        frame.render_widget(
            Paragraph::new(format!(" {marker} {label}")).style(style),
            rect,
        );
        state.people_areas.push((index, rect));
    }
}

fn terminal_area(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    let palette = Palette::default();
    let block = palette.block(false).title(Span::styled(
        format!(" {} ", state.person_name(state.user())),
        palette.style().fg(palette.text),
    ));
    let inner = block.inner(area).inner(Margin::new(1, 1));
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    let presence = state.people.get(state.selected).map_or("away".into(), |p| {
        if p.online {
            "online".into()
        } else {
            format!("away {}", idle_text(p.idle_secs))
        }
    });
    let allowed = state.may_type(state.user());
    let header = Line::from(vec![
        Span::styled(
            format!(
                "{presence}  {} terminals    ",
                state.selected_terminals().len()
            ),
            palette.style().fg(palette.subtext0),
        ),
        Span::styled(
            if allowed {
                "input: allowed"
            } else {
                "input: read only"
            },
            palette.style().fg(if allowed {
                palette.green
            } else {
                palette.subtext0
            }),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(header).style(palette.style()),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let mut content = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2),
    );
    if !state.selected_terminals().is_empty() {
        tabs(
            frame,
            state,
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
        content.y = content.y.saturating_add(1);
        content.height = content.height.saturating_sub(1);
    }
    if state.viewer.is_none() && state.people.len() == 1 && state.selected_terminals().is_empty() {
        first_run(frame, state, content);
        return;
    }
    if state.people.len() == 1
        && let Some(invite) = &state.invite
    {
        frame.render_widget(
            Paragraph::new(format!("c copy  {invite}")).style(palette.style().fg(palette.subtext0)),
            Rect::new(content.x, content.y, content.width, content.height.min(1)),
        );
        content.y = content.y.saturating_add(1);
        content.height = content.height.saturating_sub(1);
    }
    if state.viewer.is_some() {
        crate::viewer::draw(frame, state, content);
    } else {
        box_grid(frame, state, content, if area.width >= 80 { 2 } else { 1 });
    }
}

fn box_grid(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect, columns: usize) {
    state.grid_columns = columns;
    let palette = Palette::default();
    let terminals = state.selected_terminals().to_vec();
    if terminals.is_empty() {
        frame.render_widget(
            Paragraph::new(if state.user() == state.own_user {
                "No terminals.\nn new terminal"
            } else {
                "No terminals."
            })
            .style(palette.style().fg(palette.subtext0))
            .alignment(Alignment::Center),
            Rect::new(
                area.x,
                area.y + area.height.saturating_sub(2) / 2,
                area.width,
                area.height.min(2),
            ),
        );
        return;
    }
    state.focus = state.focus.min(terminals.len() - 1);
    let total_rows = terminals.len().div_ceil(columns);
    let capacity = (usize::from(area.height) + 1) / 9;
    let visible_rows = total_rows.min(if total_rows > capacity {
        usize::from(area.height) / 9
    } else {
        capacity
    });
    state.grid_scroll = state
        .grid_scroll
        .min(total_rows.saturating_sub(visible_rows));
    let remaining = terminals
        .len()
        .saturating_sub((state.grid_scroll + visible_rows) * columns);
    more(frame, area, remaining);
    let area = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(u16::from(remaining > 0)),
    );
    for (index, terminal) in terminals
        .iter()
        .enumerate()
        .skip(state.grid_scroll * columns)
        .take(visible_rows * columns)
    {
        let row = index / columns - state.grid_scroll;
        let column = index % columns;
        let x = area.x + (usize::from(area.width) * column / columns) as u16;
        let right = area.x + (usize::from(area.width) * (column + 1) / columns) as u16;
        let y = area.y + (usize::from(area.height) * row / visible_rows) as u16;
        let bottom = area.y + (usize::from(area.height) * (row + 1) / visible_rows) as u16;
        let rect = Rect::new(
            x,
            y,
            (right - x).saturating_sub(u16::from(column + 1 < columns)),
            (bottom - y).saturating_sub(u16::from(row + 1 < visible_rows)),
        );
        let mut title = Line::from(vec![
            Span::styled(
                format!(" {} ", terminal.name),
                palette.style().fg(if terminal.name == "shell" {
                    palette.subtext0
                } else {
                    palette.blue
                }),
            ),
            Span::styled(
                format!("{} ", terminal.state),
                palette.style().fg(if terminal.state == "idle" {
                    palette.green
                } else {
                    palette.yellow
                }),
            ),
        ]);
        title
            .spans
            .extend(typist_span(terminal.last_typist.as_deref()));
        let block = palette
            .block(state.chrome.grid_focus && index == state.focus)
            .title(title);
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let rows = state
            .frames
            .get(&(state.user().into(), terminal.pane.clone()))
            .map_or(&[][..], |f| f.rows.as_slice());
        frame.render_widget(PaneCells::new(rows), inner);
        state.box_areas.push((index, rect));
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

pub(crate) fn typist_span(name: Option<&str>) -> Option<Span<'static>> {
    name.map(|name| {
        Span::styled(
            format!("{name} is typing "),
            Palette::default().style().fg(Palette::default().subtext0),
        )
    })
}

use crate::{state::ClientState, theme::Palette};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

const HEADER_ROWS: u16 = 2;
const IDENTITY_ROWS: u16 = 2;
const MIN_FULL_HEIGHT: u16 = 11;

pub(super) fn compact(area: Rect) -> bool {
    area.width < 2 || area.height < MIN_FULL_HEIGHT
}

pub(super) fn column(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    if area.width < 2 || area.height == 0 {
        return;
    }
    separator(frame, state, area);
    let width = area.width - 1;
    let heading_area = Rect::new(area.x, area.y, width, 1);
    heading(frame, state, heading_area);
    state.chrome.close_area = heading_area;
    search_field(frame, state, Rect::new(area.x, area.y + 1, width, 1));
    let full = !compact(area);
    let status = if full {
        status_lines(state)
    } else {
        Vec::new()
    };
    let gap = u16::from(!status.is_empty());
    let reserved = if full {
        IDENTITY_ROWS + status.len() as u16 + gap
    } else {
        0
    };
    let list = Rect::new(
        area.x,
        area.y + HEADER_ROWS,
        width,
        area.height.saturating_sub(HEADER_ROWS + reserved),
    );
    let used = rows(frame, state, list, width);
    if !full {
        return;
    }
    if !status.is_empty() {
        let height = status.len() as u16;
        frame.render_widget(
            Paragraph::new(status).style(Palette::default().style()),
            Rect::new(area.x, list.y + used + gap, width, height),
        );
    }
    identity(
        frame,
        state,
        Rect::new(area.x, area.bottom() - IDENTITY_ROWS, width, IDENTITY_ROWS),
    );
}

fn separator(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let palette = Palette::default();
    let style = palette.style().fg(if state.chrome.grid_focus {
        palette.overlay0
    } else {
        palette.accent
    });
    let buffer = frame.buffer_mut();
    let x = area.right() - 1;
    for y in area.y..area.bottom() {
        buffer[(x, y)].set_symbol("\u{2502}").set_style(style);
    }
}

pub(super) fn status_lines(state: &ClientState) -> Vec<Line<'static>> {
    let palette = Palette::default();
    let dim = palette.style().fg(palette.subtext0);
    let user = state
        .viewer
        .as_ref()
        .map_or(state.user(), |viewer| viewer.user.as_str());
    let mut lines = Vec::new();
    if user != state.own_user {
        lines.push(if state.may_type(user) {
            Line::styled(" allowed", palette.style().fg(palette.green))
        } else {
            Line::styled(" read only", dim)
        });
    }
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

fn identity(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let palette = Palette::default();
    let server = format!(" {}", state.server);
    let server = if server.chars().count() as u16 <= area.width {
        server
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(" {}", state.own_name)),
            Line::from(server),
        ])
        .style(palette.style().fg(palette.overlay0)),
        area,
    );
}

fn heading(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let palette = Palette::default();
    let style = palette.style().fg(palette.overlay0);
    let title = " people";
    let used = title.chars().count() as u16;
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            title,
            style.add_modifier(Modifier::BOLD),
        )))
        .style(style),
        area,
    );
    frame.render_widget(
        Paragraph::new("<").style(style),
        Rect::new(area.right().saturating_sub(1), area.y, 1, 1),
    );
    let count = format!(
        "{} online",
        state.people.iter().filter(|person| person.online).count()
    );
    let width = count.chars().count() as u16;
    if width > area.width.saturating_sub(used.saturating_add(3)) {
        return;
    }
    frame.render_widget(
        Paragraph::new(count).style(style),
        Rect::new(area.right().saturating_sub(width + 2), area.y, width, 1),
    );
}

fn search_field(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    if area.height == 0 {
        return;
    }
    let palette = Palette::default();
    let (text, color) = if state.searching || !state.search.is_empty() {
        (format!(" /{}", state.search), palette.accent)
    } else {
        (" / find".to_owned(), palette.overlay0)
    };
    frame.render_widget(Paragraph::new(text).style(palette.style().fg(color)), area);
    state.chrome.search_area = area;
}

fn rows(frame: &mut Frame<'_>, state: &mut ClientState, list: Rect, width: u16) -> u16 {
    if list.height == 0 {
        return 0;
    }
    let palette = Palette::default();
    let matches = state.matches();
    let visible = usize::from(list.height)
        .saturating_sub(usize::from(matches.len() > usize::from(list.height)));
    state.people_scroll = state
        .people_scroll
        .min(matches.len().saturating_sub(visible));
    let remaining = matches.len().saturating_sub(state.people_scroll + visible);
    super::more(frame, list, remaining);
    let mut used = 0;
    for (row, index) in matches
        .into_iter()
        .skip(state.people_scroll)
        .take(visible)
        .enumerate()
    {
        let person = &state.people[index];
        let own = person.user_id == state.own_user;
        let label = if own { "you" } else { &person.name };
        let viewing = state
            .viewer
            .as_ref()
            .is_some_and(|viewer| viewer.user == person.user_id);
        let mut style = palette.style().fg(if person.online {
            palette.text
        } else {
            palette.subtext0
        });
        if index == state.selected {
            style = style.bg(palette.surface0);
        }
        if viewing {
            style = style.add_modifier(Modifier::BOLD);
        }
        let rect = Rect::new(list.x, list.y + row as u16, width, 1);
        frame.render_widget(Paragraph::new(format!(" {label}")).style(style), rect);
        state.people_areas.push((index, rect));
        used = row as u16 + 1;
    }
    used + u16::from(remaining > 0)
}

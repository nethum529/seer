use crate::{state::ClientState, theme::Palette};
use ratatui::{Frame, layout::Rect, style::Modifier, widgets::Paragraph};

pub(super) fn column(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    if area.width < 2 || area.height == 0 {
        return;
    }
    let palette = Palette::default();
    let separator = palette.style().fg(if state.chrome.grid_focus {
        palette.overlay0
    } else {
        palette.accent
    });
    let buffer = frame.buffer_mut();
    let separator_x = area.right() - 1;
    for y in area.y..area.bottom() {
        buffer[(separator_x, y)]
            .set_symbol("│")
            .set_style(separator);
    }
    let width = area.width - 1;
    let title = if state.search.is_empty() && !state.searching {
        " people".into()
    } else {
        format!(" people /{}", state.search)
    };
    frame.render_widget(
        Paragraph::new(title).style(
            palette
                .style()
                .fg(palette.overlay0)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(area.x, area.y, width, 1),
    );
    let list = Rect::new(area.x, area.y + 1, width, area.height.saturating_sub(1));
    let matches = state.matches();
    let visible = usize::from(list.height)
        .saturating_sub(usize::from(matches.len() > usize::from(list.height)));
    state.people_scroll = state
        .people_scroll
        .min(matches.len().saturating_sub(visible));
    let remaining = matches.len().saturating_sub(state.people_scroll + visible);
    super::more(frame, list, remaining);
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
        let mut style = palette.style().fg(if own {
            palette.yellow
        } else if person.online {
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
    }
}

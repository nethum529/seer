use crate::{render::idle_text, state::ClientState, theme::Palette, tui::send, viewer::Viewer};
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};
use seer_core::proto::ClientMsg;
use seer_net::Stream;
use std::io;

pub(crate) struct PersonMenu {
    user: String,
    anchor: Position,
    selected: usize,
    scroll: usize,
    area: Rect,
    rows: Vec<(usize, Rect)>,
}

pub(crate) fn open(state: &mut ClientState, index: usize, row: Rect) {
    state.select_person(index);
    if state.user() == state.own_user {
        state.menu = None;
        return;
    }
    state.menu = Some(PersonMenu {
        user: state.user().into(),
        anchor: Position::new(row.right().saturating_add(1), row.y + 1),
        selected: 0,
        scroll: 0,
        area: Rect::default(),
        rows: Vec::new(),
    });
}

pub(crate) fn key(
    key: KeyEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<()> {
    let Some(mut menu) = state.menu.take() else {
        return Ok(());
    };
    let count = state.terminals.get(&menu.user).map_or(0, Vec::len);
    match key.code {
        KeyCode::Esc => return Ok(()),
        KeyCode::Char(' ') => toggle(stream, state, &menu.user)?,
        KeyCode::Char('j') | KeyCode::Down => {
            menu.selected = (menu.selected + 1).min(count);
            reveal(&mut menu, count);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            menu.selected = menu.selected.saturating_sub(1);
            reveal(&mut menu, count);
        }
        KeyCode::Enter if activate(stream, state, &menu, count)? => return Ok(()),
        _ => {}
    }
    state.menu = Some(menu);
    Ok(())
}

fn toggle(stream: &mut impl Stream, state: &ClientState, user: &str) -> io::Result<()> {
    send(
        stream,
        &ClientMsg::SetGrant {
            user: user.into(),
            can_type: !state.can_type_here.contains(user),
        },
    )
}

fn activate(
    stream: &mut impl Stream,
    state: &mut ClientState,
    menu: &PersonMenu,
    count: usize,
) -> io::Result<bool> {
    if menu.selected >= count {
        toggle(stream, state, &menu.user)?;
        return Ok(false);
    }
    if let Some(terminal) = state
        .terminals
        .get(&menu.user)
        .and_then(|list| list.get(menu.selected))
    {
        state.viewer = Some(Viewer::new(menu.user.clone(), terminal.pane.clone()));
        state.focus = menu.selected;
        crate::panels::close(state);
        return Ok(true);
    }
    Ok(false)
}

fn reveal(menu: &mut PersonMenu, count: usize) {
    let row = action_row(menu.selected, count);
    let height = usize::from(menu.area.height.saturating_sub(2)).max(1);
    if row < menu.scroll {
        menu.scroll = row;
    }
    if row >= menu.scroll + height {
        menu.scroll = row + 1 - height;
    }
}

fn action_row(selected: usize, count: usize) -> usize {
    if selected < count {
        count + 2 + selected
    } else {
        2 * count + 3
    }
}

pub(crate) fn mouse(
    mouse: MouseEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<()> {
    let Some(mut menu) = state.menu.take() else {
        return Ok(());
    };
    let position = Position::new(mouse.column, mouse.row);
    let count = state.terminals.get(&menu.user).map_or(0, Vec::len);
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        if !menu.area.contains(position) {
            return Ok(());
        }
        if let Some((index, _)) = menu.rows.iter().find(|(_, area)| area.contains(position)) {
            menu.selected = *index;
            if activate(stream, state, &menu, count)? {
                return Ok(());
            }
        }
    } else if menu.area.contains(position) {
        match mouse.kind {
            MouseEventKind::ScrollDown => menu.scroll = menu.scroll.saturating_add(1),
            MouseEventKind::ScrollUp => menu.scroll = menu.scroll.saturating_sub(1),
            _ => {}
        }
    }
    state.menu = Some(menu);
    Ok(())
}

pub(crate) fn draw(frame: &mut Frame<'_>, state: &mut ClientState) {
    let Some(mut menu) = state.menu.take() else {
        return;
    };
    let palette = Palette::default();
    let full = frame.area();
    let lines = lines(state, &menu.user);
    let count = state.terminals.get(&menu.user).map_or(0, Vec::len);
    menu.selected = menu.selected.min(count);
    let width = full
        .width
        .min((lines.iter().map(Line::width).max().unwrap_or(0) + 4).max(22) as u16);
    let x = menu.anchor.x.min(full.right().saturating_sub(width));
    let y = menu.anchor.y.min(full.bottom().saturating_sub(4));
    let height = (lines.len().saturating_add(2) as u16)
        .min(full.bottom().saturating_sub(y).saturating_sub(1));
    menu.area = Rect::new(x, y, width, height);
    let block = palette
        .block(false)
        .style(palette.style().bg(palette.surface0));
    let inner = block.inner(menu.area);
    palette.clear(frame.buffer_mut(), menu.area);
    frame.render_widget(block, menu.area);
    menu.scroll = menu
        .scroll
        .min(lines.len().saturating_sub(usize::from(inner.height)));
    menu.rows.clear();
    for (row, (index, line)) in lines
        .into_iter()
        .enumerate()
        .skip(menu.scroll)
        .take(usize::from(inner.height))
        .enumerate()
    {
        let rect = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
        let action = (0..=count).find(|selected| action_row(*selected, count) == index);
        let style = if action == Some(menu.selected) {
            palette.style().bg(palette.surface0).fg(palette.accent)
        } else {
            palette.style().bg(palette.surface0)
        };
        frame.render_widget(Paragraph::new(line).style(style), rect);
        if let Some(action) = action {
            menu.rows.push((action, rect));
        }
    }
    state.menu = Some(menu);
}

fn lines(state: &ClientState, user: &str) -> Vec<Line<'static>> {
    let palette = Palette::default();
    let person = state.people.iter().find(|p| p.user_id == user);
    let online = person.is_some_and(|p| p.online);
    let idle = person.map_or(0, |p| p.idle_secs);
    let mut lines = vec![Line::styled(
        format!(
            "{}  {} ago",
            if online { "online" } else { "away" },
            idle_text(idle)
        ),
        palette.style().bg(palette.surface0).fg(palette.subtext0),
    )];
    let terminals = state.terminals.get(user).map_or(&[][..], Vec::as_slice);
    for terminal in terminals {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", terminal.name),
                palette
                    .style()
                    .bg(palette.surface0)
                    .fg(if terminal.name == "shell" {
                        palette.subtext0
                    } else {
                        palette.blue
                    }),
            ),
            Span::styled(
                terminal.state.clone(),
                palette
                    .style()
                    .bg(palette.surface0)
                    .fg(if terminal.state == "idle" {
                        palette.green
                    } else {
                        palette.yellow
                    }),
            ),
        ]));
    }
    lines.push(Line::styled(
        "--------------------",
        palette.style().bg(palette.surface0).fg(palette.overlay0),
    ));
    for terminal in terminals {
        lines.push(Line::from(format!("watch {}", terminal.name)));
    }
    lines.push(Line::styled(
        "--------------------",
        palette.style().bg(palette.surface0).fg(palette.overlay0),
    ));
    lines.push(Line::from(format!(
        "[{}] can type here",
        if state.can_type_here.contains(user) {
            "x"
        } else {
            " "
        }
    )));
    lines
}

pub(crate) struct TerminalMenu {
    anchor: Position,
    area: Rect,
    selected: usize,
}

pub(crate) fn open_context(state: &mut ClientState, anchor: Position) {
    state.chrome.context = Some(TerminalMenu {
        anchor,
        area: Rect::default(),
        selected: 0,
    });
}

pub(crate) fn context_key(
    key: KeyEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<()> {
    let Some(mut menu) = state.chrome.context.take() else {
        return Ok(());
    };
    match key.code {
        KeyCode::Esc => return Ok(()),
        KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('k') | KeyCode::Up => {
            menu.selected = 1 - menu.selected
        }
        KeyCode::Enter => {
            context_action(stream, state, menu.selected)?;
            return Ok(());
        }
        _ => {}
    }
    state.chrome.context = Some(menu);
    Ok(())
}

fn context_action(
    stream: &mut impl Stream,
    state: &mut ClientState,
    index: usize,
) -> io::Result<()> {
    if index == 0 {
        state.open_focused();
        Ok(())
    } else {
        state.request_close(stream)
    }
}

pub(crate) fn context_mouse(
    mouse: MouseEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<()> {
    let Some(menu) = state.chrome.context.take() else {
        return Ok(());
    };
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        let position = Position::new(mouse.column, mouse.row);
        if !menu.area.contains(position) {
            return Ok(());
        }
        let row = mouse.row.saturating_sub(menu.area.y + 1);
        if row < 2 {
            context_action(stream, state, usize::from(row))?;
            return Ok(());
        }
    }
    state.chrome.context = Some(menu);
    Ok(())
}

pub(crate) fn draw_context(frame: &mut Frame<'_>, state: &mut ClientState) {
    let Some(menu) = &mut state.chrome.context else {
        return;
    };
    let palette = Palette::default();
    let full = frame.area();
    let width = full.width.min(12);
    let height = full.height.min(4);
    menu.area = Rect::new(
        menu.anchor.x.min(full.right().saturating_sub(width)),
        menu.anchor.y.min(full.bottom().saturating_sub(height)),
        width,
        height,
    );
    palette.clear(frame.buffer_mut(), menu.area);
    let block = palette
        .block(false)
        .style(palette.style().bg(palette.surface0));
    let inner = block.inner(menu.area);
    frame.render_widget(block, menu.area);
    for (index, text) in ["Open", "Close"]
        .into_iter()
        .enumerate()
        .take(usize::from(inner.height))
    {
        let style = palette
            .style()
            .bg(palette.surface0)
            .fg(if index == menu.selected {
                palette.accent
            } else {
                palette.text
            });
        frame.render_widget(
            Paragraph::new(text).style(style),
            Rect::new(inner.x, inner.y + index as u16, inner.width, 1),
        );
    }
}

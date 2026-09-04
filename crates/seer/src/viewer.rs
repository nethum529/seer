use crate::{
    input::key_to_input,
    render::footer,
    state::ClientState,
    terminal_cells::PaneCells,
    theme::Palette,
    tui::{send, send_viewer_input},
    tui_navigation::step,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
};
use seer_core::proto::ClientMsg;
use seer_core::{TerminalFrame, TerminalInput};
use seer_net::Stream;
use std::io;

pub(crate) struct Viewer {
    pub(crate) user: String,
    pub(crate) pane: String,
    pub(crate) follow: bool,
    pub(crate) frozen: Option<TerminalFrame>,
}

impl Viewer {
    pub(crate) fn new(user: String, pane: String) -> Self {
        Self {
            user,
            pane,
            follow: true,
            frozen: None,
        }
    }
    pub(crate) fn target(&self) -> (String, String) {
        (self.user.clone(), self.pane.clone())
    }
}

pub(crate) fn key(
    key: KeyEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<()> {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, KeyModifiers::NONE) => {
            state.viewer = None;
        }
        (KeyCode::Tab, KeyModifiers::NONE) => next(state),
        (KeyCode::Char('f'), KeyModifiers::NONE) => {
            if let Some(viewer) = &mut state.viewer {
                viewer.follow = !viewer.follow;
                viewer.frozen = if viewer.follow {
                    None
                } else {
                    state.frames.get(&viewer.target()).cloned()
                };
            }
        }
        _ => {
            if let Some(input) = key_to_input(key) {
                input_message(stream, state, input)?;
            }
        }
    }
    Ok(())
}

fn next(state: &mut ClientState) {
    let Some(viewer) = &state.viewer else {
        return;
    };
    let Some(terminals) = state
        .terminals
        .get(&viewer.user)
        .filter(|list| !list.is_empty())
    else {
        return;
    };
    let index = terminals
        .iter()
        .position(|t| t.pane == viewer.pane)
        .unwrap_or(0);
    let index = step(index, terminals.len(), true);
    state.viewer = Some(Viewer::new(
        viewer.user.clone(),
        terminals[index].pane.clone(),
    ));
    state.focus = index;
}

pub(crate) fn input_message(
    stream: &mut impl Stream,
    state: &ClientState,
    input: TerminalInput,
) -> io::Result<()> {
    let Some(viewer) = &state.viewer else {
        return Ok(());
    };
    if viewer.user == state.own_user {
        return send_viewer_input(stream, state, input);
    }
    if state.you_may_type_into.contains(&viewer.user)
        && let Some(bytes) = crate::input::raw_bytes(&input)?
    {
        send(
            stream,
            &ClientMsg::TypeInto {
                user: viewer.user.clone(),
                pane: viewer.pane.clone(),
                bytes,
            },
        )?;
    }
    Ok(())
}

pub(crate) fn draw(frame: &mut Frame<'_>, state: &ClientState) {
    let Some(viewer) = &state.viewer else {
        return;
    };
    let palette = Palette::default();
    let area = frame.area();
    let area = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    let name = state
        .terminals
        .get(&viewer.user)
        .into_iter()
        .flatten()
        .find(|t| t.pane == viewer.pane)
        .map_or("shell", |t| t.name.as_str());
    let allowed = state.may_type(&viewer.user);
    let mode = if allowed { "input" } else { "read only" };
    let title = Line::from(vec![
        Span::styled(
            format!(" {}  ", state.person_name(&viewer.user)),
            palette.style(),
        ),
        Span::styled(
            format!("{name}  "),
            palette.style().fg(if name == "shell" {
                palette.subtext0
            } else {
                palette.blue
            }),
        ),
        Span::styled(
            format!("{mode} "),
            palette.style().fg(if allowed {
                palette.green
            } else {
                palette.subtext0
            }),
        ),
    ]);
    let block = palette.block(true).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let content = if viewer.follow {
        state.frames.get(&viewer.target())
    } else {
        viewer.frozen.as_ref()
    };
    if let Some(content) = content {
        frame.render_widget(PaneCells::new(&content.rows), inner);
        let start = content.rows.len().saturating_sub(usize::from(inner.height));
        if allowed
            && viewer.follow
            && content.cursor.visible
            && let Some(row) = usize::from(content.cursor.row).checked_sub(start)
            && row < usize::from(inner.height)
            && content.cursor.column < inner.width
        {
            frame.set_cursor_position((inner.x + content.cursor.column, inner.y + row as u16));
        }
    }
    let hints = format!(
        "esc back  tab next terminal  f follow {}",
        if viewer.follow { "on" } else { "off" }
    );
    footer(
        frame,
        if state.notice.is_empty() {
            &hints
        } else {
            &state.notice
        },
    );
}

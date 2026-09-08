use crate::{
    input::key_to_input,
    state::ClientState,
    terminal_cells::{PaneCells, start_row},
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
use seer_core::TerminalInput;
use seer_core::proto::ClientMsg;
use seer_net::Stream;
use std::io;

pub(crate) struct Viewer {
    pub(crate) user: String,
    pub(crate) pane: String,
    pub(crate) area: Rect,
    history: Vec<Vec<seer_core::Cell>>,
    previous: Vec<Vec<seer_core::Cell>>,
    pub(crate) offset: usize,
}

impl Viewer {
    pub(crate) fn new(user: String, pane: String) -> Self {
        Self {
            user,
            pane,
            area: Rect::default(),
            history: Vec::new(),
            previous: Vec::new(),
            offset: 0,
        }
    }
    pub(crate) fn scroll(&mut self, up: bool) {
        self.offset = if up {
            (self.offset + 3).min(self.history.len())
        } else {
            self.offset.saturating_sub(3)
        };
    }

    fn note_rows(&mut self, rows: &[Vec<seer_core::Cell>]) {
        if self.previous == rows {
            return;
        }
        let overlap = (1..self.previous.len()).find(|shift| {
            let suffix = &self.previous[*shift..];
            rows.starts_with(suffix)
        });
        if let Some(shift) = overlap {
            self.history.extend_from_slice(&self.previous[..shift]);
            if self.offset > 0 {
                self.offset += shift;
            }
            let excess = self.history.len().saturating_sub(2000);
            self.history.drain(..excess);
            self.offset = self.offset.min(self.history.len());
        }
        self.previous = rows.to_vec();
    }

    pub(crate) fn visible_rows(&self, height: u16) -> Vec<Vec<seer_core::Cell>> {
        self.history
            .iter()
            .chain(&self.previous)
            .skip(self.history.len().saturating_sub(self.offset))
            .take(usize::from(height))
            .cloned()
            .collect()
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

pub(crate) fn draw(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    if let Some(viewer) = &mut state.viewer {
        viewer.area = area;
        if let Some(content) = state.frames.get(&viewer.target()) {
            viewer.note_rows(&content.rows);
        }
    }
    let Some(viewer) = &state.viewer else {
        return;
    };
    let palette = Palette::default();
    let terminal = state
        .terminals
        .get(&viewer.user)
        .into_iter()
        .flatten()
        .find(|t| t.pane == viewer.pane);
    let name = terminal.map_or("shell", |t| t.name.as_str());
    let allowed = state.may_type(&viewer.user);
    let mode = if allowed { "input" } else { "read only" };
    let mut title = Line::from(vec![
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
    title.spans.extend(crate::render::typist_span(
        terminal.and_then(|t| t.last_typist.as_deref()),
    ));
    let area = viewer.area;
    let block = palette.block(true).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let content = state.frames.get(&viewer.target());
    if let Some(content) = content {
        let rows = viewer.visible_rows(inner.height);
        let start = start_row(&rows, inner.height);
        frame.render_widget(PaneCells::new(&rows), inner);
        if allowed
            && viewer.offset == 0
            && content.cursor.visible
            && let Some(row) = usize::from(content.cursor.row).checked_sub(start)
            && row < usize::from(inner.height)
            && content.cursor.column < inner.width
        {
            frame.set_cursor_position((inner.x + content.cursor.column, inner.y + row as u16));
        }
    }
}

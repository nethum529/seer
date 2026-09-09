use crate::{
    input::key_to_input,
    state::ClientState,
    terminal_cells::{PaneCells, start_row},
    tui::{send, send_viewer_input},
};
use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Rect, Size},
};
use seer_core::TerminalInput;
use seer_core::proto::ClientMsg;
use std::io;

pub(crate) struct Viewer {
    pub(crate) user: String,
    pub(crate) pane: String,
    pub(crate) area: Rect,
    pub(crate) sent_size: Option<Size>,
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
            sent_size: None,
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
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<()> {
    if let Some(input) = key_to_input(key) {
        input_message(stream, state, input)?;
    }
    Ok(())
}

pub(crate) fn input_message(
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
    input: TerminalInput,
) -> io::Result<()> {
    if state.chrome_owns_input() {
        return Ok(());
    }
    if state.viewer.is_none()
        && matches!(
            input.event,
            seer_core::InputEvent::Key(_)
                | seer_core::InputEvent::Text(_)
                | seer_core::InputEvent::Paste(_)
        )
    {
        state.open_focused();
    }
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
    let allowed = state.may_type(&viewer.user);
    let area = viewer.area;
    if area.is_empty() {
        return;
    }
    if let Some(content) = state.frames.get(&viewer.target()) {
        let rows = viewer.visible_rows(area.height);
        let start = start_row(&rows, area.height);
        frame.render_widget(PaneCells::new(&rows), area);
        if allowed
            && !state.chrome_owns_input()
            && viewer.offset == 0
            && content.cursor.visible
            && let Some(row) = usize::from(content.cursor.row).checked_sub(start)
            && row < usize::from(area.height)
            && content.cursor.column < area.width
        {
            frame.set_cursor_position((area.x + content.cursor.column, area.y + row as u16));
        }
    }
}

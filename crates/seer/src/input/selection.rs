use crate::{state::ClientState, theme::Palette};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
};
use seer_core::Cell;
use std::io::{self, Write};

pub(crate) struct Selection {
    area: Rect,
    rows: Vec<Vec<Cell>>,
    anchor: usize,
    end: usize,
    dragged: bool,
}

impl Selection {
    fn index(&self, position: Position) -> usize {
        let x = position.x.clamp(self.area.x, self.area.right() - 1) - self.area.x;
        let y = position.y.clamp(self.area.y, self.area.bottom() - 1) - self.area.y;
        usize::from(y) * usize::from(self.area.width) + usize::from(x)
    }

    fn text(&self) -> String {
        let width = usize::from(self.area.width);
        let start = self.anchor.min(self.end);
        let end = self.anchor.max(self.end);
        (start / width..=end / width)
            .map(|row| {
                let left = if row == start / width {
                    start % width
                } else {
                    0
                };
                let right = if row == end / width {
                    end % width + 1
                } else {
                    width
                };
                let text: String = (left..right)
                    .map(|column| {
                        self.rows
                            .get(row)
                            .and_then(|line| line.get(column))
                            .map_or(' ', |cell| cell.character)
                    })
                    .collect();
                text.trim_end().to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) fn draw(&self, buffer: &mut Buffer) {
        if !self.dragged {
            return;
        }
        let width = usize::from(self.area.width);
        for index in self.anchor.min(self.end)..=self.anchor.max(self.end) {
            let x = self.area.x + (index % width) as u16;
            let y = self.area.y + (index / width) as u16;
            if let Some(cell) = buffer.cell_mut(Position::new(x, y)) {
                let character = self
                    .rows
                    .get(index / width)
                    .and_then(|row| row.get(index % width))
                    .map_or(' ', |cell| cell.character);
                cell.set_char(character).set_bg(Palette::default().surface1);
            }
        }
    }
}

pub(super) fn mouse(mouse: MouseEvent, state: &mut ClientState) -> io::Result<bool> {
    let position = Position::new(mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::Down(_) => {
            state.selection = None;
            if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                && state.menu.is_none()
                && state.chrome.context.is_none()
            {
                state.selection = begin(state, position);
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(selection) = &mut state.selection {
                selection.end = selection.index(position);
                selection.dragged |= selection.end != selection.anchor;
            }
            return Ok(true);
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if let Some(selection) = state.selection.take()
                && selection.dragged
            {
                copy_text(&selection.text())?;
                state.set_notice("Copied");
                return Ok(true);
            }
        }
        _ => {}
    }
    Ok(false)
}

fn begin(state: &ClientState, position: Position) -> Option<Selection> {
    let (area, rows) = if let Some(viewer) = &state.viewer {
        let area = viewer.content;
        (area, viewer.visible_rows(area.height))
    } else {
        let tile = state
            .box_areas
            .iter()
            .find(|tile| tile.placed.contains(position))?;
        let terminal = state.selected_terminals().get(tile.index)?;
        let area = tile.placed;
        let rows = &state
            .frames
            .get(&(state.user().into(), terminal.pane.clone()))?
            .rows;
        (area, rows.clone())
    };
    if area.is_empty() || !area.contains(position) {
        return None;
    }
    let mut selection = Selection {
        area,
        rows,
        anchor: 0,
        end: 0,
        dragged: false,
    };
    selection.anchor = selection.index(position);
    selection.end = selection.anchor;
    Some(selection)
}

pub(crate) fn copy_text(text: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    write!(stdout, "\x1b]52;c;{}\x07", base64(text.as_bytes()))?;
    stdout.flush()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::new();
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        text.push(char::from(ALPHABET[usize::from(first >> 2)]));
        text.push(char::from(
            ALPHABET[usize::from(((first & 3) << 4) | (second >> 4))],
        ));
        text.push(if chunk.len() > 1 {
            char::from(ALPHABET[usize::from(((second & 15) << 2) | (third >> 6))])
        } else {
            '='
        });
        text.push(if chunk.len() > 2 {
            char::from(ALPHABET[usize::from(third & 63)])
        } else {
            '='
        });
    }
    text
}

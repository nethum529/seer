use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect, Size};
use ratatui::widgets::{Block, Borders, Clear};

use crate::render::draw_tree;
use crate::state::ClientState;

const HANDLE_WIDTH: u16 = 1;
const MINIMUM_DRAWER_WIDTH: u16 = 40;

#[derive(Default)]
pub(crate) struct Drawer {
    open: bool,
}

impl Drawer {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub(crate) fn close_on_escape(&mut self, key: KeyEvent) -> bool {
        if self.open && key.code == KeyCode::Esc {
            self.open = false;
            return true;
        }
        false
    }

    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent, size: Size) -> bool {
        let area = Rect::new(0, 0, size.width, size.height);
        let position = Position::new(mouse.column, mouse.row);
        if handle_area(area).contains(position) {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                self.toggle();
            }
            return true;
        }
        self.open && drawer_area(area).contains(position)
    }
}

pub(crate) fn pane_size(size: Size) -> Size {
    Size::new(size.width.saturating_sub(HANDLE_WIDTH), size.height)
}

pub(crate) fn draw(
    frame: &mut Frame<'_>,
    state: &mut ClientState,
    drawer: &Drawer,
    status: &str,
    peek_person: Option<&str>,
) {
    let full_area = frame.area();
    draw_tree(frame, state, pane_area(full_area), status, peek_person);
    frame.render_widget(
        Block::default().borders(Borders::LEFT),
        handle_area(full_area),
    );
    if drawer.is_open() {
        let area = drawer_area(full_area);
        frame.render_widget(Clear, area);
        frame.render_widget(Block::default().borders(Borders::ALL).title("People"), area);
    }
}

fn pane_area(mut area: Rect) -> Rect {
    area.width = area.width.saturating_sub(HANDLE_WIDTH);
    area
}

fn handle_area(area: Rect) -> Rect {
    let width = area.width.min(HANDLE_WIDTH);
    Rect::new(
        area.x + area.width.saturating_sub(width),
        area.y,
        width,
        area.height,
    )
}

fn drawer_area(area: Rect) -> Rect {
    let pane = pane_area(area);
    let width = (area.width / 3).max(MINIMUM_DRAWER_WIDTH).min(pane.width);
    Rect::new(
        pane.x + pane.width.saturating_sub(width),
        pane.y,
        width,
        pane.height,
    )
}

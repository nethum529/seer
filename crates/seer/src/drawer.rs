use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect, Size};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::render::PaneCells;
use crate::state::{ClientState, pane_rects};

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
    let mut area = pane_area(full_area);
    let status_height = area.height.min(1);
    let status_area = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(status_height),
        area.width,
        status_height,
    );
    frame.render_widget(Paragraph::new(status), status_area);
    area.height = area.height.saturating_sub(status_height);
    if let Some(person) = peek_person {
        let banner_height = area.height.min(2);
        let banner = Rect::new(area.x, area.y, area.width, banner_height);
        frame.render_widget(
            Paragraph::new(format!(
                "PEEK: {person} - READ ONLY\nWorkspace: {person}/{}",
                state.selected_workspace().unwrap_or("unknown")
            )),
            banner,
        );
        area.y = area.y.saturating_add(banner_height);
        area.height = area.height.saturating_sub(banner_height);
    }
    draw_panes(frame, state, area);
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

fn draw_panes(frame: &mut Frame<'_>, state: &mut ClientState, area: Rect) {
    let Some(tab) = state.visible_tab().cloned() else {
        state.set_pane_areas(Vec::new());
        return;
    };
    let mut input_areas = Vec::new();
    for (pane, pane_area) in pane_rects(&tab, area) {
        let block = Block::default().borders(Borders::ALL).title(pane.as_str());
        let inner = block.inner(pane_area);
        frame.render_widget(block, pane_area);
        frame.render_widget(PaneCells::new(state.pane_rows(&pane)), inner);
        set_frame_cursor(frame, state, &pane, inner);
        input_areas.push((pane, inner));
    }
    state.set_pane_areas(input_areas);
}

fn set_frame_cursor(frame: &mut Frame<'_>, state: &ClientState, pane: &str, area: Rect) {
    let Some(cursor) = state
        .pane_cursor(pane)
        .filter(|cursor| cursor.visible && state.focused() == Some(pane))
    else {
        return;
    };
    if cursor.column < area.width && cursor.row < area.height {
        frame.set_cursor_position((area.x + cursor.column, area.y + cursor.row));
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

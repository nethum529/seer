use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect, Size};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use seer_core::proto::{Person, PersonState};

use crate::preview::Preview;
use crate::render::{PaneCells, draw_tree};
use crate::state::ClientState;

const HANDLE_WIDTH: u16 = 1;
const MINIMUM_DRAWER_WIDTH: u16 = 40;

#[derive(Default)]
pub(crate) struct Drawer {
    open: bool,
    people: Vec<Person>,
    selected: usize,
    preview: Option<Preview>,
    preview_target: Option<String>,
}

impl Drawer {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub(crate) fn set_people(&mut self, people: Vec<Person>) {
        self.people = people;
        self.selected = self.selected.min(self.people.len().saturating_sub(1));
    }

    pub(crate) fn tick_preview(&mut self) -> bool {
        self.sync_preview();
        match self.preview.as_mut().map(Preview::tick) {
            Some(Ok(redraw)) => redraw,
            Some(Err(_)) => {
                self.preview = None;
                true
            }
            None => false,
        }
    }

    fn sync_preview(&mut self) {
        let wanted = self.highlighted().map(|person| person.user_id.clone());
        if wanted == self.preview_target {
            return;
        }
        self.preview = None;
        self.preview_target = wanted;
        if let Some(person) = self.highlighted() {
            self.preview = Preview::open(person).ok();
        }
    }

    fn highlighted(&self) -> Option<&Person> {
        if !self.open {
            return None;
        }
        self.people
            .get(self.selected)
            .filter(|person| person.peekable)
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.open {
            return false;
        }
        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.people.len().saturating_sub(1));
            }
            KeyCode::Enter => {}
            KeyCode::Esc => self.open = false,
            _ => return false,
        }
        true
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
        draw_drawer(frame, drawer, drawer_area(full_area));
    }
}

fn draw_drawer(frame: &mut Frame<'_>, drawer: &Drawer, area: Rect) {
    frame.render_widget(Clear, area);
    let list_height = list_height(drawer, area);
    let list = Rect::new(area.x, area.y, area.width, list_height);
    let block = Block::default().borders(Borders::ALL).title("People");
    let inner = block.inner(list);
    frame.render_widget(block, list);
    frame.render_widget(Paragraph::new(people_lines(drawer, inner.width)), inner);
    draw_preview(
        frame,
        drawer,
        Rect::new(
            area.x,
            area.y.saturating_add(list_height),
            area.width,
            area.height.saturating_sub(list_height),
        ),
    );
}

fn list_height(drawer: &Drawer, area: Rect) -> u16 {
    if drawer.preview.is_none() {
        return area.height;
    }
    let rows = u16::try_from(drawer.people.len()).unwrap_or(u16::MAX);
    rows.saturating_add(2).min(area.height / 2).min(area.height)
}

fn draw_preview(frame: &mut Frame<'_>, drawer: &Drawer, area: Rect) {
    let (Some(preview), Some(person)) = (drawer.preview.as_ref(), drawer.highlighted()) else {
        return;
    };
    if area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(format!("{} live", person.name)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    frame.render_widget(
        PaneCells::new(preview.rows()),
        Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(1),
        ),
    );
}

fn people_lines(drawer: &Drawer, width: u16) -> Vec<Line<'static>> {
    drawer
        .people
        .iter()
        .enumerate()
        .map(|(index, person)| {
            let line = Line::from(person_row(person, width));
            if index == drawer.selected {
                return line.style(Style::default().add_modifier(Modifier::REVERSED));
            }
            line
        })
        .collect()
}

fn person_row(person: &Person, width: u16) -> String {
    let dot = match person.state {
        PersonState::Active => '*',
        PersonState::Idle => 'o',
        PersonState::Away => '.',
    };
    let foreground = if person.foreground.is_empty() {
        "-"
    } else {
        person.foreground.as_str()
    };
    let row = format!(
        "{dot} {:<12} {:<12} {:>4}",
        person.name,
        foreground,
        idle_text(person.idle_secs)
    );
    format!("{row:<width$}", width = usize::from(width))
}

fn idle_text(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3600)
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

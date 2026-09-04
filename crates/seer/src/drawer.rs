use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect, Size};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use seer_core::proto::{Person, PersonState};

use crate::render::draw_tree;
use crate::state::ClientState;

const ROW_WIDTH: u16 = 32;
const DRAWER_WIDTH: u16 = ROW_WIDTH + 2;
const HINT: &str = "Enter peek  Esc close";

#[derive(Default)]
pub(crate) struct Drawer {
    open: bool,
    own_user: String,
    people: Vec<Person>,
    selected: usize,
    pending_peek: Option<Person>,
}

pub(crate) enum DrawerAction {
    Ignored,
    Handled,
    Peek(Person),
}

impl Drawer {
    pub(crate) fn new(own_user: String) -> Self {
        Self {
            own_user,
            ..Self::default()
        }
    }

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

    fn is_own(&self, person: &Person) -> bool {
        person.user_id == self.own_user
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> DrawerAction {
        if !self.open {
            return DrawerAction::Ignored;
        }
        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.people.len().saturating_sub(1));
            }
            KeyCode::Enter => return self.take_highlighted_peek(),
            KeyCode::Esc => self.open = false,
            _ => return DrawerAction::Ignored,
        }
        DrawerAction::Handled
    }

    fn take_highlighted_peek(&mut self) -> DrawerAction {
        let Some(person) = self
            .people
            .get(self.selected)
            .filter(|person| person.peekable && !self.is_own(person))
        else {
            return DrawerAction::Handled;
        };
        let person = person.clone();
        self.open = false;
        DrawerAction::Peek(person)
    }

    pub(crate) fn set_pending_peek(&mut self, person: Person) {
        self.pending_peek = Some(person);
    }

    pub(crate) fn take_pending_peek(&mut self) -> Option<Person> {
        self.pending_peek.take()
    }

    pub(crate) fn peek_pending(&self) -> bool {
        self.pending_peek.is_some()
    }

    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent, size: Size) -> bool {
        let area = Rect::new(0, 0, size.width, size.height);
        let position = Position::new(mouse.column, mouse.row);
        if button_area(area, self.people.len()).contains(position) {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                self.toggle();
            }
            return true;
        }
        self.open && drawer_area(area).contains(position)
    }
}

pub(crate) fn draw(
    frame: &mut Frame<'_>,
    state: &mut ClientState,
    drawer: &Drawer,
    status: &str,
    peek_person: Option<&str>,
) {
    let full_area = frame.area();
    draw_tree(frame, state, full_area, status, peek_person);
    if drawer.is_open() {
        draw_drawer(frame, drawer, drawer_area(full_area));
    }
    draw_button(frame, drawer, full_area);
}

fn draw_button(frame: &mut Frame<'_>, drawer: &Drawer, area: Rect) {
    let button = button_area(area, drawer.people.len());
    if button.is_empty() {
        return;
    }
    frame.render_widget(Clear, button);
    frame.render_widget(Paragraph::new(button_text(drawer.people.len())), button);
}

fn draw_drawer(frame: &mut Frame<'_>, drawer: &Drawer, area: Rect) {
    frame.render_widget(Clear, area);
    let block = Block::default().borders(Borders::ALL).title("People");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(people_lines(drawer, inner.width)),
        Rect::new(inner.x, inner.y, inner.width, inner.height - 1),
    );
    frame.render_widget(
        Paragraph::new(HINT),
        Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
    );
}

fn people_lines(drawer: &Drawer, width: u16) -> Vec<Line<'static>> {
    drawer
        .people
        .iter()
        .enumerate()
        .map(|(index, person)| {
            let line = Line::from(person_row(person, drawer.is_own(person), width));
            if index == drawer.selected {
                return line.style(Style::default().add_modifier(Modifier::REVERSED));
            }
            line
        })
        .collect()
}

fn person_row(person: &Person, own: bool, width: u16) -> String {
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
    let idle = if own {
        "you".to_owned()
    } else {
        idle_text(person.idle_secs)
    };
    let row = format!("{dot} {:<12} {:<12} {idle:>4}", person.name, foreground);
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

fn button_text(people: usize) -> String {
    format!("[ People {people} ]")
}

fn button_area(area: Rect, people: usize) -> Rect {
    let text = u16::try_from(button_text(people).len()).unwrap_or(u16::MAX);
    let width = text.min(area.width);
    Rect::new(
        area.x + area.width.saturating_sub(width),
        area.y,
        width,
        area.height.min(1),
    )
}

fn drawer_area(area: Rect) -> Rect {
    let width = DRAWER_WIDTH.min(area.width);
    Rect::new(
        area.x + area.width.saturating_sub(width),
        area.y,
        width,
        area.height,
    )
}

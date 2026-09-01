use std::cell::{Cell as ModeCell, RefCell};
use std::io::{self, Stdout};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{Cell, Color, Tree};

use crate::input::key_to_bytes;
use crate::state::{ClientState, pane_rects};

const EVENT_WAIT: Duration = Duration::from_millis(25);

thread_local! {
    static VIEW_ONLY: ModeCell<bool> = const { ModeCell::new(false) };
    static PEEK_PERSON: RefCell<Option<String>> = const { RefCell::new(None) };
}

enum ReaderEvent {
    Message(ServerMsg),
    Failed(io::Error),
}

#[derive(Debug, PartialEq)]
enum LoopControl {
    Continue,
    Exit,
    Detached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionExit {
    Client,
    Detached,
}

pub(crate) fn run(mut stream: TcpStream, tree: Tree) -> io::Result<SessionExit> {
    let mut terminal = TerminalSession::start()?;
    let reader = stream.try_clone()?;
    let (receiver, reader_thread) = spawn_reader(reader);
    let loop_result = run_loop(&mut terminal.terminal, &mut stream, &receiver, tree);
    drop(terminal);
    let _ = stream.shutdown(Shutdown::Both);
    join_reader(reader_thread)?;
    loop_result.map(|control| match control {
        LoopControl::Detached => SessionExit::Detached,
        LoopControl::Continue | LoopControl::Exit => SessionExit::Client,
    })
}

fn spawn_reader(mut stream: TcpStream) -> (Receiver<ReaderEvent>, JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        loop {
            match codec::decode(&mut stream) {
                Ok(message) => {
                    if sender.send(ReaderEvent::Message(message)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = sender.send(ReaderEvent::Failed(error));
                    break;
                }
            }
        }
    });
    (receiver, handle)
}

fn join_reader(reader: JoinHandle<()>) -> io::Result<()> {
    reader
        .join()
        .map_err(|_| io::Error::other("socket reader thread panicked"))
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    stream: &mut TcpStream,
    receiver: &Receiver<ReaderEvent>,
    tree: Tree,
) -> io::Result<LoopControl> {
    let mut state = ClientState::new(tree);
    let mut command_pending = false;
    let mut dirty = true;

    loop {
        let received = receive_messages(receiver, &mut state, &mut dirty)?;
        if received != LoopControl::Continue {
            return Ok(received);
        }
        if dirty {
            terminal.draw(|frame| draw(frame, &state))?;
            dirty = false;
        }
        if event::poll(EVENT_WAIT)? {
            let event = event::read()?;
            if let LoopControl::Exit =
                handle_event(event, stream, &mut state, &mut command_pending)?
            {
                return Ok(LoopControl::Exit);
            }
            dirty = true;
        }
    }
}

fn receive_messages(
    receiver: &Receiver<ReaderEvent>,
    state: &mut ClientState,
    dirty: &mut bool,
) -> io::Result<LoopControl> {
    loop {
        match receiver.try_recv() {
            Ok(ReaderEvent::Message(message)) => {
                let control = apply_server_message(message, state)?;
                if control != LoopControl::Continue {
                    return Ok(control);
                }
                *dirty = true;
            }
            Ok(ReaderEvent::Failed(error)) => return Err(error),
            Err(TryRecvError::Empty) => return Ok(LoopControl::Continue),
            Err(TryRecvError::Disconnected) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "server connection closed",
                ));
            }
        }
    }
}

fn apply_server_message(message: ServerMsg, state: &mut ClientState) -> io::Result<LoopControl> {
    match message {
        ServerMsg::Tree { tree } => state.replace_tree(tree),
        ServerMsg::Cells { pane, rows } => state.apply_cells(pane, rows),
        ServerMsg::Bye { reason } if reason == "detached" => {
            return Ok(LoopControl::Detached);
        }
        ServerMsg::Bye { .. } => return Ok(LoopControl::Exit),
        ServerMsg::Frame { .. } => {}
        ServerMsg::Welcome { .. }
        | ServerMsg::Joined { .. }
        | ServerMsg::Seat { .. }
        | ServerMsg::People { .. }
        | ServerMsg::Clients { .. }
        | ServerMsg::Refused { .. } => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected handshake message",
            ));
        }
    }
    Ok(LoopControl::Continue)
}

fn handle_event(
    event: Event,
    stream: &mut TcpStream,
    state: &mut ClientState,
    command_pending: &mut bool,
) -> io::Result<LoopControl> {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            handle_key(key, stream, state, command_pending)
        }
        Event::Resize(cols, rows) if !is_view_only() => {
            send(stream, &ClientMsg::Resize { cols, rows })?;
            Ok(LoopControl::Continue)
        }
        _ => Ok(LoopControl::Continue),
    }
}

fn handle_key(
    key: KeyEvent,
    stream: &mut TcpStream,
    state: &mut ClientState,
    command_pending: &mut bool,
) -> io::Result<LoopControl> {
    if is_control_char(key, 'q') {
        send(stream, &ClientMsg::Detach)?;
        return Ok(LoopControl::Exit);
    }
    if is_view_only() {
        return Ok(LoopControl::Continue);
    }
    if is_control_char(key, 'b') {
        *command_pending = true;
        return Ok(LoopControl::Continue);
    }
    if *command_pending {
        *command_pending = false;
        if let KeyCode::Char(number @ '1'..='9') = key.code {
            let index = number.to_digit(10).map_or(0, |value| value as usize);
            if let Some(pane) = state.focus_number(index) {
                send(stream, &ClientMsg::FocusPane { pane })?;
            }
            return Ok(LoopControl::Continue);
        }
    }
    if let (Some(pane), Some(bytes)) = (state.focused(), key_to_bytes(key)) {
        send(
            stream,
            &ClientMsg::Input {
                pane: pane.to_owned(),
                bytes,
            },
        )?;
    }
    Ok(LoopControl::Continue)
}

fn is_control_char(key: KeyEvent, character: char) -> bool {
    key.code == KeyCode::Char(character) && key.modifiers.contains(KeyModifiers::CONTROL)
}

#[cfg(test)]
pub(crate) fn set_view_only(view_only: bool) {
    VIEW_ONLY.set(view_only);
    if !view_only {
        PEEK_PERSON.set(None);
    }
}

pub(crate) fn set_peek_person(person: Option<&str>) {
    VIEW_ONLY.set(person.is_some());
    PEEK_PERSON.set(person.map(str::to_owned));
}

fn is_view_only() -> bool {
    VIEW_ONLY.get()
}

fn peek_person() -> Option<String> {
    PEEK_PERSON.with_borrow(Clone::clone)
}

fn send(stream: &mut TcpStream, message: &ClientMsg) -> io::Result<()> {
    codec::encode(stream, message)
}

fn draw(frame: &mut ratatui::Frame<'_>, state: &ClientState) {
    let mut area = frame.area();
    if let Some(person) = peek_person() {
        let banner_height = area.height.min(2);
        let banner = Rect::new(area.x, area.y, area.width, banner_height);
        frame.render_widget(
            Paragraph::new(format!(
                "PEEK: {person} - READ ONLY\nWorkspace: {person}/current"
            )),
            banner,
        );
        area.y = area.y.saturating_add(banner_height);
        area.height = area.height.saturating_sub(banner_height);
    }
    let Some(tab) = state.visible_tab() else {
        return;
    };
    for (pane, pane_area) in pane_rects(tab, area) {
        let block = Block::default().borders(Borders::ALL).title(pane.as_str());
        let inner = block.inner(pane_area);
        frame.render_widget(block, pane_area);
        frame.render_widget(PaneCells::new(state.pane_rows(&pane)), inner);
    }
}

struct PaneCells<'a> {
    rows: &'a [Vec<Cell>],
}

impl<'a> PaneCells<'a> {
    fn new(rows: &'a [Vec<Cell>]) -> Self {
        Self { rows }
    }
}

impl Widget for PaneCells<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        for (row_index, row) in self.rows.iter().take(area.height as usize).enumerate() {
            let y = area.y.saturating_add(row_index as u16);
            for (column_index, cell) in row.iter().take(area.width as usize).enumerate() {
                let x = area.x.saturating_add(column_index as u16);
                buffer[(x, y)]
                    .set_char(cell.character)
                    .set_style(cell_style(cell));
            }
        }
    }
}

fn cell_style(cell: &Cell) -> Style {
    let style = Style::default().fg(color(cell.fg));
    if cell.bold {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn color(color: Color) -> ratatui::style::Color {
    match color {
        Color::Default => ratatui::style::Color::Reset,
        Color::Indexed(index) => ratatui::style::Color::Indexed(index),
        Color::Rgb { red, green, blue } => ratatui::style::Color::Rgb(red, green, blue),
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn start() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests;

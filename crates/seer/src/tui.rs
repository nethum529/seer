use std::cell::{Cell as ModeCell, RefCell};
use std::io::{self, Stdout};
use std::net::Shutdown;
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
use seer_net::{Socket, Stream};

use crate::input::{
    FocusDirection, InputAction, key_to_action, pane_in_direction as find_pane_in_direction,
    selected_tab_tree,
};
use crate::state::{ClientState, pane_rects};

const EVENT_WAIT: Duration = Duration::from_millis(25);
const STATUS_HINT: &str = "Ctrl-b c new tab, % split, x close, n/p tabs";
const LAST_TAB_STATUS: &str = "Cannot close the last tab.";
thread_local! {
    static VIEW_ONLY: ModeCell<bool> = const { ModeCell::new(false) };
    static PEEK_PERSON: RefCell<Option<String>> = const { RefCell::new(None) };
    static NAVIGATION_TREE: RefCell<Option<Tree>> = const { RefCell::new(None) };
    static STATUS: RefCell<&'static str> = const { RefCell::new(STATUS_HINT) };
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

pub(crate) fn run(mut stream: Socket, tree: Tree) -> io::Result<SessionExit> {
    let mut terminal = TerminalSession::start()?;
    let reader = stream.clone();
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

fn spawn_reader(mut stream: Socket) -> (Receiver<ReaderEvent>, JoinHandle<()>) {
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

fn run_loop<S: Stream>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    stream: &mut S,
    receiver: &Receiver<ReaderEvent>,
    tree: Tree,
) -> io::Result<LoopControl> {
    set_navigation_tree(&tree);
    set_status(STATUS_HINT);
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
        ServerMsg::Tree { tree } => {
            set_navigation_tree(&tree);
            state.replace_tree(tree);
        }
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

fn handle_event<S: Stream>(
    event: Event,
    stream: &mut S,
    state: &mut ClientState,
    command_pending: &mut bool,
) -> io::Result<LoopControl> {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            handle_key(key, stream, state, command_pending)
        }
        Event::Resize(cols, rows) if !is_view_only() => {
            if let Some((workspace, tab)) = state.selection() {
                send(
                    stream,
                    &ClientMsg::Resize {
                        workspace: workspace.to_owned(),
                        tab: tab.to_owned(),
                        cols,
                        rows,
                    },
                )?;
            }
            Ok(LoopControl::Continue)
        }
        _ => Ok(LoopControl::Continue),
    }
}

fn handle_key<S: Stream>(
    key: KeyEvent,
    stream: &mut S,
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
    if let Some(action) = key_to_action(key, command_pending) {
        set_status(STATUS_HINT);
        handle_action(action, stream, state)?;
    }
    Ok(LoopControl::Continue)
}

fn handle_action(
    action: InputAction,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<()> {
    let message = match action {
        InputAction::CreateTab => {
            state
                .selected_workspace()
                .map(|workspace| ClientMsg::CreateTab {
                    workspace: workspace.to_owned(),
                })
        }
        InputAction::SplitPane(direction) => {
            state
                .selection()
                .map(|(workspace, tab)| ClientMsg::SplitPane {
                    workspace: workspace.to_owned(),
                    tab: tab.to_owned(),
                    direction,
                })
        }
        InputAction::ClosePane => close_message(state),
        InputAction::NextTab => {
            select_tab(state, true);
            None
        }
        InputAction::PreviousTab => {
            select_tab(state, false);
            None
        }
        InputAction::FocusPane(direction) => {
            pane_in_direction(state, direction).and_then(|pane| focus_message(state, pane))
        }
        InputAction::FocusNumber(number) => state
            .focus_number(number)
            .and_then(|pane| focus_message(state, pane)),
        InputAction::Bytes(bytes) => {
            state
                .selection()
                .zip(state.focused())
                .map(|((workspace, tab), pane)| ClientMsg::Input {
                    workspace: workspace.to_owned(),
                    tab: tab.to_owned(),
                    pane: pane.to_owned(),
                    bytes,
                })
        }
    };
    if let Some(message) = message {
        send(stream, &message)?;
    }
    Ok(())
}

fn close_message(state: &ClientState) -> Option<ClientMsg> {
    if closes_last_tab(state) {
        set_status(LAST_TAB_STATUS);
        return None;
    }
    state
        .selection()
        .zip(state.focused())
        .map(|((workspace, tab), pane)| ClientMsg::ClosePane {
            workspace: workspace.to_owned(),
            tab: tab.to_owned(),
            pane: pane.to_owned(),
        })
}

fn focus_message(state: &ClientState, pane: String) -> Option<ClientMsg> {
    state
        .selection()
        .map(|(workspace, tab)| ClientMsg::FocusPane {
            workspace: workspace.to_owned(),
            tab: tab.to_owned(),
            pane,
        })
}

fn select_tab(state: &mut ClientState, forward: bool) {
    let Some((workspace, tab)) = state
        .selection()
        .map(|(workspace, tab)| (workspace.to_owned(), tab.to_owned()))
    else {
        return;
    };
    let selected = NAVIGATION_TREE.with_borrow(|tree| {
        tree.as_ref()
            .and_then(|tree| selected_tab_tree(tree, &workspace, &tab, forward))
    });
    if let Some(tree) = selected {
        state.replace_tree(tree);
    }
}

fn closes_last_tab(state: &ClientState) -> bool {
    let Some((workspace, _)) = state.selection() else {
        return false;
    };
    let last_pane = state.visible_tab().is_some_and(|tab| tab.panes.len() == 1);
    last_pane
        && NAVIGATION_TREE.with_borrow(|tree| {
            tree.as_ref().is_none_or(|tree| {
                tree.workspaces
                    .iter()
                    .find(|candidate| candidate.id == workspace)
                    .is_none_or(|workspace| workspace.tabs.len() == 1)
            })
        })
}

fn pane_in_direction(state: &ClientState, direction: FocusDirection) -> Option<String> {
    let tab = state.visible_tab()?;
    let focused = state.focused()?;
    let rects = pane_rects(tab, Rect::new(0, 0, 1_000, 1_000));
    find_pane_in_direction(&rects, focused, direction)
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

fn set_navigation_tree(tree: &Tree) {
    NAVIGATION_TREE.with_borrow_mut(|current| *current = Some(tree.clone()));
}

fn set_status(status: &'static str) {
    STATUS.with_borrow_mut(|current| *current = status);
}

fn status() -> &'static str {
    STATUS.with_borrow(|status| *status)
}

fn send(stream: &mut impl Stream, message: &ClientMsg) -> io::Result<()> {
    codec::encode(stream, message)
}

fn draw(frame: &mut ratatui::Frame<'_>, state: &ClientState) {
    let mut area = frame.area();
    let status_height = area.height.min(1);
    let status_area = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(status_height),
        area.width,
        status_height,
    );
    frame.render_widget(Paragraph::new(status()), status_area);
    area.height = area.height.saturating_sub(status_height);
    if let Some(person) = peek_person() {
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

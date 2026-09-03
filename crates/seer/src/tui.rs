use std::cell::{Cell as ModeCell, RefCell};
use std::io::{self, Stdout};
use std::net::Shutdown;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalInput, Tree};
use seer_net::{Socket, Stream};

use crate::input::{
    InputAction, is_control_char, key_to_action, mouse_event_is_tracked, mouse_to_input,
};
use crate::render::PaneCells;
use crate::state::{ClientState, pane_rects};
use crate::terminal_session::{TerminalSession, ignore_setup_disconnect, set_cursor_style};
use crate::tui_navigation::{
    closes_last_tab, initialize, pane_in_direction, select_tab, show_hint, show_last_tab_status,
    status, update_tree,
};

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

pub(crate) fn run(mut stream: Socket, tree: Tree) -> io::Result<SessionExit> {
    let mut terminal = TerminalSession::start()?;
    initialize(&tree);
    let state = ClientState::new(tree);
    send_terminal_setup(&mut stream, &terminal.terminal, &state)?;
    let reader = stream.clone();
    let (receiver, reader_thread) = spawn_reader(reader);
    let loop_result = run_loop(&mut terminal.terminal, &mut stream, &receiver, state);
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
    mut state: ClientState,
) -> io::Result<LoopControl> {
    let mut command_pending = false;
    let mut dirty = true;

    loop {
        let received = receive_messages(receiver, &mut state, &mut dirty)?;
        if received != LoopControl::Continue {
            return Ok(received);
        }
        if dirty {
            terminal.draw(|frame| draw(frame, &mut state))?;
            set_cursor_style(&state)?;
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
            update_tree(&tree);
            state.replace_tree(tree);
        }
        ServerMsg::Cells { pane, frame } => state.apply_frame(pane, frame),
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
        Event::Mouse(mouse) if !is_view_only() => handle_mouse(mouse, stream, state),
        Event::Paste(text) if !is_view_only() => {
            send_focused_input(stream, state, TerminalInput::new(InputEvent::Paste(text)))?;
            Ok(LoopControl::Continue)
        }
        Event::FocusGained if !is_view_only() => {
            send_focused_input(stream, state, TerminalInput::new(InputEvent::Focus(true)))?;
            Ok(LoopControl::Continue)
        }
        Event::FocusLost if !is_view_only() => {
            send_focused_input(stream, state, TerminalInput::new(InputEvent::Focus(false)))?;
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
        show_hint();
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
        InputAction::Bytes(input) => {
            state
                .selection()
                .zip(state.focused())
                .map(|((workspace, tab), pane)| ClientMsg::TerminalInput {
                    workspace: workspace.to_owned(),
                    tab: tab.to_owned(),
                    pane: pane.to_owned(),
                    input,
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
        show_last_tab_status();
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

fn handle_mouse<S: Stream>(
    mouse: MouseEvent,
    stream: &mut S,
    state: &mut ClientState,
) -> io::Result<LoopControl> {
    let Some((pane, column, row)) = state.mouse_target(mouse.column, mouse.row) else {
        return Ok(LoopControl::Continue);
    };
    if !mouse_event_is_tracked(mouse.kind, state.pane_mouse_tracking(&pane)) {
        return Ok(LoopControl::Continue);
    }
    let Some((workspace, tab)) = state
        .selection()
        .map(|(workspace, tab)| (workspace.to_owned(), tab.to_owned()))
    else {
        return Ok(LoopControl::Continue);
    };
    if matches!(mouse.kind, MouseEventKind::Down(_)) && state.focused() != Some(&pane) {
        state.set_focus(pane.clone());
        send(
            stream,
            &ClientMsg::FocusPane {
                workspace: workspace.clone(),
                tab: tab.clone(),
                pane: pane.clone(),
            },
        )?;
    }
    let input = mouse_to_input(mouse, column, row);
    send(
        stream,
        &ClientMsg::TerminalInput {
            workspace,
            tab,
            pane,
            input,
        },
    )?;
    Ok(LoopControl::Continue)
}

fn send_focused_input(
    stream: &mut impl Stream,
    state: &ClientState,
    input: TerminalInput,
) -> io::Result<()> {
    if let (Some((workspace, tab)), Some(pane)) = (state.selection(), state.focused()) {
        send(
            stream,
            &ClientMsg::TerminalInput {
                workspace: workspace.to_owned(),
                tab: tab.to_owned(),
                pane: pane.to_owned(),
                input,
            },
        )?;
    }
    Ok(())
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

fn send(stream: &mut impl Stream, message: &ClientMsg) -> io::Result<()> {
    codec::encode(stream, message)
}

fn send_terminal_setup<S: Stream>(
    stream: &mut S,
    terminal: &Terminal<CrosstermBackend<Stdout>>,
    state: &ClientState,
) -> io::Result<()> {
    let capabilities = TerminalCapabilities {
        protocol_version: TERMINAL_PROTOCOL_VERSION,
    };
    if let Err(error) = send(stream, &ClientMsg::TerminalCapabilities { capabilities }) {
        return ignore_setup_disconnect(error);
    }
    if is_view_only() {
        return Ok(());
    }
    let Some((workspace, tab)) = state.selection() else {
        return Ok(());
    };
    let size = terminal.size()?;
    if let Err(error) = send(
        stream,
        &ClientMsg::Resize {
            workspace: workspace.to_owned(),
            tab: tab.to_owned(),
            cols: size.width,
            rows: size.height,
        },
    ) {
        return ignore_setup_disconnect(error);
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame<'_>, state: &mut ClientState) {
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

fn set_frame_cursor(frame: &mut ratatui::Frame<'_>, state: &ClientState, pane: &str, area: Rect) {
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

#[cfg(test)]
mod tests;

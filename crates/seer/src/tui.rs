use std::io::{self, Stdout};
use std::net::Shutdown;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Size;
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalInput, Tree};
use seer_net::{Socket, Stream};

use crate::drawer::{self, Drawer};
use crate::input::{
    InputAction, is_control_char, key_to_action, mouse_event_is_tracked, mouse_to_input,
};
pub(crate) use crate::peek_mode::set_peek_person;
#[cfg(test)]
pub(crate) use crate::peek_mode::set_view_only;
use crate::peek_mode::{self, is_view_only, peek_person};
use crate::state::ClientState;
use crate::terminal_session::{TerminalSession, ignore_setup_disconnect, set_cursor_style};
use crate::tui_navigation::{
    closes_last_tab, forward_prefix, initialize, pane_in_direction, select_tab, show_hint,
    show_last_tab_status, status, update_tree,
};

const EVENT_WAIT: Duration = Duration::from_millis(25);

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

pub(crate) fn run(mut stream: Socket, tree: Tree, own_user: String) -> io::Result<SessionExit> {
    let mut terminal = TerminalSession::start()?;
    initialize(&tree);
    let state = ClientState::new(tree, own_user);
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
    let mut drawer = Drawer::default();

    loop {
        let size = terminal.size()?;
        let received =
            receive_messages(receiver, &mut state, &mut dirty, stream, size, &mut drawer)?;
        if received != LoopControl::Continue {
            return Ok(received);
        }
        if drawer.tick_preview() {
            dirty = true;
        }
        if dirty {
            terminal.draw(|frame| draw(frame, &mut state, &drawer))?;
            set_cursor_style(&state)?;
            dirty = false;
        }
        if event::poll(EVENT_WAIT)? {
            let event = event::read()?;
            let size = terminal.size()?;
            if let LoopControl::Exit = handle_event(
                event,
                stream,
                &mut state,
                &mut command_pending,
                size,
                &mut drawer,
            )? {
                return Ok(LoopControl::Exit);
            }
            dirty = true;
        }
    }
}

fn receive_messages<S: Stream>(
    receiver: &Receiver<ReaderEvent>,
    state: &mut ClientState,
    dirty: &mut bool,
    stream: &mut S,
    size: Size,
    drawer: &mut Drawer,
) -> io::Result<LoopControl> {
    loop {
        match receiver.try_recv() {
            Ok(ReaderEvent::Message(message)) => {
                let control = apply_server_message(message, state, stream, size, drawer)?;
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

fn apply_server_message<S: Stream>(
    message: ServerMsg,
    state: &mut ClientState,
    stream: &mut S,
    size: Size,
    drawer: &mut Drawer,
) -> io::Result<LoopControl> {
    match message {
        ServerMsg::Tree { tree } => {
            update_tree(&tree);
            if state.replace_tree(tree) && !is_view_only() {
                send_resize(stream, state, size)?;
            }
        }
        ServerMsg::Cells { pane, frame } => state.apply_frame(pane, frame),
        ServerMsg::Bye { reason } if reason == "detached" => {
            return Ok(LoopControl::Detached);
        }
        ServerMsg::Bye { .. } => return Ok(LoopControl::Exit),
        ServerMsg::People { people } => {
            state.note_people(&people);
            drawer.set_people(people);
        }
        ServerMsg::Targets { targets } if drawer.peek_pending() => {
            peek_mode::start(stream, drawer, &targets)?;
        }
        ServerMsg::Refused { reason } if drawer.peek_pending() => peek_mode::refuse(drawer, reason),
        ServerMsg::Frame { .. } => {}
        ServerMsg::Welcome { .. }
        | ServerMsg::Joined { .. }
        | ServerMsg::Seat { .. }
        | ServerMsg::Status { .. }
        | ServerMsg::Clients { .. }
        | ServerMsg::Targets { .. }
        | ServerMsg::Refused { .. }
        | ServerMsg::RuntimeReady { .. } => {
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
    size: Size,
    drawer: &mut Drawer,
) -> io::Result<LoopControl> {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            handle_key(key, stream, state, command_pending, size, drawer)
        }
        Event::Resize(cols, rows) if !is_view_only() => {
            send_resize(stream, state, Size::new(cols, rows))?;
            Ok(LoopControl::Continue)
        }
        Event::Mouse(mouse) => handle_mouse(mouse, stream, state, size, drawer),
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
    size: Size,
    drawer: &mut Drawer,
) -> io::Result<LoopControl> {
    if is_control_char(key, 'q') {
        send(stream, &ClientMsg::Detach)?;
        return Ok(LoopControl::Exit);
    }
    if peek_mode::handle_drawer_key(key, stream, drawer)? {
        return Ok(LoopControl::Continue);
    }
    let action = key_to_action(key, command_pending);
    if is_view_only() {
        if matches!(action, Some(InputAction::ToggleDrawer)) {
            toggle_drawer(stream, drawer)?;
        }
        return Ok(LoopControl::Continue);
    }
    if let Some(action) = action {
        if state.herdr_in_front()
            && !matches!(action, InputAction::ToggleDrawer | InputAction::Bytes(_))
        {
            forward_prefix(key, stream, state)?;
            return Ok(LoopControl::Continue);
        }
        show_hint();
        handle_action(action, stream, state, size, drawer)?;
    }
    Ok(LoopControl::Continue)
}

fn handle_action(
    action: InputAction,
    stream: &mut impl Stream,
    state: &mut ClientState,
    size: Size,
    drawer: &mut Drawer,
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
        InputAction::NextTab | InputAction::PreviousTab => {
            let forward = action == InputAction::NextTab;
            select_tab(state, forward);
            return send_resize(stream, state, size);
        }
        InputAction::ToggleDrawer => return toggle_drawer(stream, drawer),
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

fn toggle_drawer(stream: &mut impl Stream, drawer: &mut Drawer) -> io::Result<()> {
    drawer.toggle();
    if drawer.is_open() {
        return send(stream, &ClientMsg::ListPeople);
    }
    Ok(())
}

fn send_resize(stream: &mut impl Stream, state: &ClientState, size: Size) -> io::Result<()> {
    let Some((workspace, tab)) = state.selection() else {
        return Ok(());
    };
    send(
        stream,
        &ClientMsg::Resize {
            workspace: workspace.to_owned(),
            tab: tab.to_owned(),
            cols: drawer::pane_size(size).width,
            rows: size.height,
        },
    )
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
    size: Size,
    drawer: &mut Drawer,
) -> io::Result<LoopControl> {
    let was_open = drawer.is_open();
    if drawer.handle_mouse(mouse, size) || is_view_only() {
        if drawer.is_open() && !was_open {
            send(stream, &ClientMsg::ListPeople)?;
        }
        return Ok(LoopControl::Continue);
    }
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

pub(crate) fn send_focused_input(
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

pub(crate) fn send(stream: &mut impl Stream, message: &ClientMsg) -> io::Result<()> {
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
    if let Err(error) = send_resize(stream, state, terminal.size()?) {
        return ignore_setup_disconnect(error);
    }
    Ok(())
}

fn draw(frame: &mut ratatui::Frame<'_>, state: &mut ClientState, drawer: &Drawer) {
    let person = peek_person();
    let notice = peek_mode::take_notice();
    let line = notice.as_deref().unwrap_or(status());
    drawer::draw(frame, state, drawer, line, person.as_deref());
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod herdr_tests;

#[cfg(test)]
mod tests;

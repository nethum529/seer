use crate::{
    render,
    state::ClientState,
    terminal_session::{TerminalSession, ignore_setup_disconnect, set_cursor_style},
    tui_navigation as navigation,
};
use crossterm::event::{self, Event, KeyEventKind};
pub(crate) use navigation::set_peek_person;
use ratatui::{Terminal, backend::CrosstermBackend, layout::Size};
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalInput, Tree};
use seer_net::{Socket, Stream};
use std::io::{self, Stdout};
use std::net::Shutdown;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionExit {
    Client,
    Detached,
}

pub(crate) fn run(mut stream: Socket, tree: Tree, own_user: String) -> io::Result<SessionExit> {
    let mut terminal = TerminalSession::start()?;
    let mut state = ClientState::new(tree, own_user);
    if let Ok(store) = crate::store::ServerStore::load()
        && let Some(server) = store
            .servers
            .iter()
            .find(|s| s.user_id == state.own_user && s.current)
            .or_else(|| store.servers.iter().find(|s| s.user_id == state.own_user))
    {
        state.server.clone_from(&server.endpoint);
        state.own_name.clone_from(&server.name);
    }
    let mut start_person = navigation::take_start_person();
    send(
        &mut stream,
        &ClientMsg::TerminalCapabilities {
            capabilities: TerminalCapabilities {
                protocol_version: TERMINAL_PROTOCOL_VERSION,
            },
        },
    )
    .or_else(ignore_setup_disconnect)?;
    send(&mut stream, &ClientMsg::ListPeople).or_else(ignore_setup_disconnect)?;
    send(
        &mut stream,
        &ClientMsg::Terminals {
            user: state.own_user.clone(),
        },
    )
    .or_else(ignore_setup_disconnect)?;
    let (receiver, reader) = spawn_reader(stream.clone());
    let result = run_loop(
        &mut terminal.terminal,
        &mut stream,
        &receiver,
        &mut state,
        &mut start_person,
    );
    drop(terminal);
    drop(receiver);
    let _ = stream.shutdown(Shutdown::Both);
    reader
        .join()
        .map_err(|_| io::Error::other("socket reader thread panicked"))?;
    result
}

fn spawn_reader(mut stream: Socket) -> (Receiver<io::Result<ServerMsg>>, JoinHandle<()>) {
    let (sender, receiver) = mpsc::sync_channel(64);
    let reader = thread::spawn(move || {
        loop {
            let message = codec::decode(&mut stream);
            let failed = message.is_err();
            if sender.send(message).is_err() || failed {
                break;
            }
        }
    });
    (receiver, reader)
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    stream: &mut impl Stream,
    receiver: &Receiver<io::Result<ServerMsg>>,
    state: &mut ClientState,
    start_person: &mut Option<String>,
) -> io::Result<SessionExit> {
    let mut dirty = true;
    let mut last_click = None;
    loop {
        let was_viewing = state.viewer.as_ref().map(crate::viewer::Viewer::target);
        for _ in 0..64 {
            match receiver.try_recv() {
                Ok(message) => {
                    if let Some(exit) = apply_message(message?, stream, state, start_person)? {
                        return Ok(exit);
                    }
                    dirty = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Err(io::Error::other("server connection closed"));
                }
            }
        }
        if was_viewing != state.viewer.as_ref().map(crate::viewer::Viewer::target) {
            resize(stream, state, terminal.size()?)?;
        }
        if dirty {
            sync_watches(stream, state)?;
            terminal.draw(|frame| render::draw(frame, state))?;
            set_cursor_style(state)?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(25))? {
            if handle_event(
                event::read()?,
                stream,
                state,
                terminal.size()?,
                &mut last_click,
            )? {
                return Ok(SessionExit::Client);
            }
            dirty = true;
        }
    }
}

fn apply_message(
    message: ServerMsg,
    stream: &mut impl Stream,
    state: &mut ClientState,
    start_person: &mut Option<String>,
) -> io::Result<Option<SessionExit>> {
    match message {
        ServerMsg::Grants {
            you_may_type_into,
            can_type_here,
        } => {
            state.you_may_type_into = you_may_type_into.into_iter().collect();
            state.can_type_here = can_type_here.into_iter().collect();
        }
        ServerMsg::Tree { tree } => state.replace_tree(tree),
        ServerMsg::Cells { user, pane, frame } => {
            state.frames.insert((user, pane), frame);
        }
        ServerMsg::Terminals { user, terminals } => {
            if let Some(viewer) = &state.viewer
                && viewer.user == user
            {
                if let Some(index) = terminals.iter().position(|t| t.pane == viewer.pane) {
                    state.focus = index;
                } else {
                    state.viewer = None;
                    state.notice = "Terminal closed.".into();
                }
            }
            state.terminals.insert(user, terminals);
        }
        ServerMsg::People { people } => {
            state.note_people(&people);
            if let Some(name) = start_person.as_ref()
                && let Some(index) = state.people.iter().position(|p| &p.name == name)
            {
                state.select_person(index);
                start_person.take();
            }
            send(
                stream,
                &ClientMsg::Terminals {
                    user: state.user().into(),
                },
            )?;
            if state.people.len() == 1 && state.invite.is_none() {
                navigation::invite(stream, state)?;
            }
        }
        ServerMsg::Presence {
            user,
            online,
            idle_secs,
        } => {
            if let Some(person) = state.people.iter_mut().find(|p| p.user_id == user) {
                person.online = online;
                person.idle_secs = idle_secs;
            }
        }
        ServerMsg::Seat { capsule, .. } => {
            state.invite_pending = false;
            state.invite = Some(format!("seer join {capsule}"));
        }
        ServerMsg::Refused { reason } => {
            state.notice = reason;
            state.pending_new = None;
            state.invite_pending = false;
        }
        ServerMsg::Bye { reason } => {
            return Ok(Some(if reason == "detached" {
                SessionExit::Detached
            } else {
                SessionExit::Client
            }));
        }
        _ => {}
    }
    Ok(None)
}

fn handle_event(
    event: Event,
    stream: &mut impl Stream,
    state: &mut ClientState,
    size: Size,
    last_click: &mut Option<(String, usize, Instant)>,
) -> io::Result<bool> {
    let old_user = state.user().to_owned();
    let old_viewer = state.viewer.as_ref().map(crate::viewer::Viewer::target);
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            state.notice.clear();
            if state.menu.is_some() {
                crate::person_menu::key(key, stream, state)?;
            } else if state.viewer.is_some() {
                crate::viewer::key(key, stream, state)?;
            } else if navigation::key(key, stream, state)? {
                send(stream, &ClientMsg::Detach)?;
                return Ok(true);
            }
        }
        Event::Paste(text) => crate::viewer::input_message(
            stream,
            state,
            TerminalInput::new(InputEvent::Paste(text)),
        )?,
        Event::Mouse(mouse) if state.menu.is_some() => {
            crate::person_menu::mouse(mouse, stream, state)?
        }
        Event::Mouse(mouse) if state.viewer.is_none() => {
            navigation::mouse(mouse, state, last_click)
        }
        Event::Resize(_, _) => resize(stream, state, size)?,
        _ => {}
    }
    if old_user != state.user() {
        send(
            stream,
            &ClientMsg::Terminals {
                user: state.user().into(),
            },
        )?;
    }
    if old_viewer != state.viewer.as_ref().map(crate::viewer::Viewer::target) {
        resize(stream, state, size)?;
    }
    Ok(false)
}

pub(crate) fn send_viewer_input(
    stream: &mut impl Stream,
    state: &ClientState,
    input: TerminalInput,
) -> io::Result<()> {
    let Some(viewer) = &state.viewer else {
        return Ok(());
    };
    let (user, pane) = (&viewer.user, &viewer.pane);
    if user != &state.own_user {
        return Ok(());
    }
    if let Some((workspace, tab)) = state.location(pane) {
        send(
            stream,
            &ClientMsg::TerminalInput {
                workspace,
                tab,
                pane: pane.clone(),
                input,
            },
        )?;
    }
    Ok(())
}

pub(crate) fn resize(stream: &mut impl Stream, state: &ClientState, size: Size) -> io::Result<()> {
    let Some(viewer) = &state.viewer else {
        return Ok(());
    };
    let (user, pane) = (&viewer.user, &viewer.pane);
    if user != &state.own_user {
        return Ok(());
    }
    if let Some((workspace, tab)) = state.location(pane) {
        send(
            stream,
            &ClientMsg::Resize {
                workspace,
                tab,
                cols: size.width.saturating_sub(2).max(1),
                rows: size.height.saturating_sub(3).max(1),
            },
        )?;
    }
    Ok(())
}

fn sync_watches(stream: &mut impl Stream, state: &mut ClientState) -> io::Result<()> {
    let wanted: std::collections::BTreeSet<_> = if let Some(viewer) = &state.viewer {
        [viewer.target()].into_iter().collect()
    } else {
        state
            .selected_terminals()
            .iter()
            .map(|t| (state.user().to_owned(), t.pane.clone()))
            .collect()
    };
    for (user, pane) in state.watches.difference(&wanted) {
        send(
            stream,
            &ClientMsg::Unwatch {
                user: user.clone(),
                pane: pane.clone(),
            },
        )?;
    }
    for (user, pane) in wanted.difference(&state.watches) {
        send(
            stream,
            &ClientMsg::Watch {
                user: user.clone(),
                pane: pane.clone(),
            },
        )?;
    }
    state.watches = wanted;
    Ok(())
}

pub(crate) fn send(stream: &mut impl Stream, message: &ClientMsg) -> io::Result<()> {
    codec::encode(stream, message)
}

use crate::routes::Routes;
use crate::tui_link::{
    Envelope, Events, Reconnects, Source, event_channel, join_reader, spawn_reader,
    spawn_terminal_reader,
};
use crate::{
    render,
    state::ClientState,
    terminal_session::{TerminalSession, ignore_setup_disconnect, set_cursor_style},
    tui_navigation as navigation,
    tui_sync::{sync_resize, sync_watches},
};
use crossterm::event::{Event, KeyEventKind};
pub(crate) use navigation::set_peek_person;
use ratatui::{Terminal, backend::CrosstermBackend};
use seer_core::proto::{ClientMsg, ServerMsg};
use seer_core::{InputEvent, TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalInput, Tree};
use seer_net::{Socket, Stream};
use std::io::{self, IsTerminal, Stdout};
use std::net::Shutdown;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionExit {
    Client,
    Detached,
    Restarted,
    ServerStopped,
    LocalLinkLost,
    TerminalLost,
}

pub(crate) fn run(
    local: Socket,
    room: Option<Socket>,
    tree: Tree,
    server: crate::store::ServerEntry,
    standing_notice: Option<String>,
) -> io::Result<SessionExit> {
    seer_core::debug_log!(
        "session start server={} room={}",
        server.endpoint,
        room.is_some()
    );
    let mut terminal = TerminalSession::start()?;
    let mut state = ClientState::for_server(tree, &server);
    state.set_standing_notice(standing_notice);
    let mut start_person = navigation::take_start_person();
    let mut routes = Routes::new(local.clone(), room.clone(), state.own_user.clone());
    subscribe(&mut routes, &state).or_else(ignore_setup_disconnect)?;
    let (events, sender) = event_channel();
    let local_reader = spawn_reader(local.clone(), Source::Local, sender.clone());
    let room_reader = room
        .clone()
        .map(|room| spawn_reader(room, Source::Room, sender.clone()));
    let reconnects = Reconnects::new(server, sender);
    if room.is_none() {
        reconnects.start();
    }
    let terminal_events = spawn_terminal_reader();
    let result = run_loop(
        &mut terminal.terminal,
        &mut routes,
        Inputs {
            events: &events,
            terminal_events: &terminal_events,
        },
        &reconnects,
        &mut state,
        &mut start_person,
    );
    drop(terminal);
    reconnects.stop();
    drop(events);
    let _ = local.shutdown(Shutdown::Both);
    if let Some(room) = &room {
        let _ = room.shutdown(Shutdown::Both);
    }
    join_reader(local_reader)?;
    if let Some(reader) = room_reader {
        join_reader(reader)?;
    }
    seer_core::debug_log!("session end result={result:?}");
    result
}

// Own terminals come from the local runtime; people come from the room.
fn subscribe(routes: &mut Routes, state: &ClientState) -> io::Result<()> {
    routes.send(&ClientMsg::TerminalCapabilities {
        capabilities: TerminalCapabilities {
            protocol_version: TERMINAL_PROTOCOL_VERSION,
        },
    })?;
    routes.send(&ClientMsg::ListPeople)?;
    routes.send(&ClientMsg::Terminals {
        user: state.own_user.clone(),
    })
}

struct Inputs<'a> {
    events: &'a Events,
    terminal_events: &'a Receiver<Event>,
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    stream: &mut Routes,
    inputs: Inputs<'_>,
    reconnects: &Reconnects,
    state: &mut ClientState,
    start_person: &mut Option<String>,
) -> io::Result<SessionExit> {
    let mut dirty = true;
    let mut readers = Vec::new();
    loop {
        for _ in 0..64 {
            match inputs.events.try_recv() {
                Ok(envelope) => match drain(envelope, stream, state, start_person)? {
                    Some(exit) => return Ok(exit),
                    None => dirty = true,
                },
                Err(TryRecvError::Disconnected) => return Ok(SessionExit::LocalLinkLost),
                Err(TryRecvError::Empty) => break,
            }
        }
        if stream.take_room_loss() {
            note_room_offline(state);
            reconnects.start();
            dirty = true;
        }
        if let Some(room) = reconnects.take(state) {
            readers.push(reconnects.adopt(room.clone())?);
            stream.restore_room(room);
            resubscribe(stream, state)?;
            dirty = true;
        }
        dirty |= render::expire_notice(state);
        if dirty {
            terminal.draw(|frame| render::draw(frame, state))?;
            sync_watches(stream, state)?;
            sync_resize(stream, state)?;
            set_cursor_style(state)?;
            dirty = false;
        }
        // Issue 429: after a hangup the tty answers every ioctl with EIO, so
        // isatty turns false. This is the only sign of a lost terminal for a
        // window that does not get SIGHUP or ignores it.
        if !io::stdout().is_terminal() {
            return Ok(SessionExit::TerminalLost);
        }
        match inputs
            .terminal_events
            .recv_timeout(Duration::from_millis(25))
        {
            Ok(event) => {
                if handle_event(event, stream, state)? {
                    return Ok(SessionExit::Client);
                }
                dirty = true;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(SessionExit::TerminalLost),
        }
    }
}

// The local runtime ending stops this window. The room ending does not.
fn drain(
    envelope: Envelope,
    stream: &mut Routes,
    state: &mut ClientState,
    start_person: &mut Option<String>,
) -> io::Result<Option<SessionExit>> {
    match (envelope.source, envelope.message) {
        // Only this client being detached closes the window. A room that
        // stopped takes the shared view away, not the local shells.
        (Source::Room, Ok(ServerMsg::Bye { reason })) if reason != "detached" => {
            stream.drop_room();
            Ok(None)
        }
        (_, Ok(message)) => apply_message(message, stream, state, start_person),
        (Source::Local, Err(_)) => Ok(Some(SessionExit::LocalLinkLost)),
        (Source::Room, Err(_)) => {
            stream.drop_room();
            Ok(None)
        }
    }
}

// A room that went away leaves nothing to show for the other people. Their
// terminals come back from the next fresh subscription, never from a replay.
fn note_room_offline(state: &mut ClientState) {
    let own = state.own_user.clone();
    state.terminals.retain(|user, _| user == &own);
    state.frames.retain(|(user, _), _| user == &own);
    state.seqs.clear();
    state.resyncing.clear();
    if state
        .viewer
        .as_ref()
        .is_some_and(|viewer| viewer.user != own)
    {
        state.viewer = None;
    }
    state.you_may_type_into.clear();
    state.room_was_lost = true;
    state.set_notice("Room offline. Your terminals keep running.");
}

fn resubscribe(stream: &mut Routes, state: &mut ClientState) -> io::Result<()> {
    state.watches.clear();
    if std::mem::take(&mut state.room_was_lost) {
        state.set_notice("Room back.");
    }
    stream.send(&ClientMsg::ListPeople)?;
    stream.send(&ClientMsg::Terminals {
        user: state.user().to_owned(),
    })
}

fn apply_message(
    message: ServerMsg,
    stream: &mut crate::routes::Routes,
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
        ServerMsg::Cells {
            user,
            pane,
            frame,
            seq,
        } => state.note_frame(user, pane, frame, seq),
        ServerMsg::CellsDiff {
            user,
            pane,
            seq,
            diff,
        } => {
            if state.note_diff(user.clone(), pane.clone(), seq, &diff) {
                send(stream, &ClientMsg::Resync { user, pane })?;
            }
        }
        ServerMsg::Terminals { user, terminals } => {
            if let Some(viewer) = &state.viewer
                && viewer.user == user
            {
                if let Some(index) = terminals.iter().position(|t| t.pane == viewer.pane) {
                    state.focus = index;
                } else {
                    state.viewer = None;
                    state.set_notice("Terminal closed.");
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
            state.invite = Some(crate::commands::join_line(&capsule));
        }
        ServerMsg::Refused { reason } => {
            state.set_notice(reason);
            state.pending_new = None;
            state.invite_pending = false;
        }
        ServerMsg::Bye { reason } => {
            return Ok(Some(match reason.as_str() {
                "detached" => SessionExit::Detached,
                "restarted" => SessionExit::Restarted,
                "server stopped" => SessionExit::ServerStopped,
                _ => SessionExit::Client,
            }));
        }
        _ => {}
    }
    Ok(None)
}

fn handle_event(
    event: Event,
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<bool> {
    let old_user = state.user().to_owned();
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if crate::input::command(key, stream, state)? {
                send(stream, &ClientMsg::Detach)?;
                return Ok(true);
            }
        }
        Event::Paste(text) => crate::viewer::input_message(
            stream,
            state,
            TerminalInput::new(InputEvent::Paste(text)),
        )?,
        Event::FocusGained => crate::viewer::input_message(
            stream,
            state,
            TerminalInput::new(InputEvent::Focus(true)),
        )?,
        Event::FocusLost => crate::viewer::input_message(
            stream,
            state,
            TerminalInput::new(InputEvent::Focus(false)),
        )?,
        Event::Mouse(mouse) if crate::input::mouse(mouse, stream, state)? => {
            send(stream, &ClientMsg::Detach)?;
            return Ok(true);
        }
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
    Ok(false)
}

pub(crate) fn send_viewer_input(
    stream: &mut crate::routes::Routes,
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

pub(crate) fn send(stream: &mut Routes, message: &ClientMsg) -> io::Result<()> {
    stream.send(message)
}

#[cfg(test)]
mod skew_tests;

#[cfg(test)]
mod tests;

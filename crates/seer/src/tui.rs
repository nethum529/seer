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
        dirty |= render::expire_notice(state);
        if dirty {
            terminal.draw(|frame| render::draw(frame, state))?;
            sync_watches(stream, state)?;
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
        Event::Mouse(mouse) => {
            if crate::input::mouse(mouse, stream, state, last_click)? {
                send(stream, &ClientMsg::Detach)?;
                return Ok(true);
            }
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
    let visible = if let Some(viewer) = &state.viewer {
        vec![(viewer.target(), viewer.area)]
    } else {
        state
            .box_areas
            .iter()
            .filter_map(|(index, area)| {
                state
                    .selected_terminals()
                    .get(*index)
                    .map(|terminal| ((state.user().to_owned(), terminal.pane.clone()), *area))
            })
            .collect()
    };
    let wanted: std::collections::BTreeMap<_, _> = visible
        .into_iter()
        .filter_map(|(target, area)| {
            let inner = area.inner(ratatui::layout::Margin::new(1, 1));
            (!inner.is_empty()).then_some((target, inner.as_size()))
        })
        .collect();
    for (user, pane) in state
        .watches
        .keys()
        .filter(|target| !wanted.contains_key(*target))
    {
        send(
            stream,
            &ClientMsg::Unwatch {
                user: user.clone(),
                pane: pane.clone(),
            },
        )?;
    }
    for ((user, pane), size) in &wanted {
        if state.watches.get(&(user.clone(), pane.clone())) != Some(size) {
            send(
                stream,
                &ClientMsg::Watch {
                    user: user.clone(),
                    pane: pane.clone(),
                    cols: size.width,
                    rows: size.height,
                },
            )?;
        }
    }
    state.watches = wanted;
    Ok(())
}

pub(crate) fn send(stream: &mut impl Stream, message: &ClientMsg) -> io::Result<()> {
    codec::encode(stream, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use seer_core::proto::TerminalInfo;
    use std::os::unix::net::UnixStream;

    #[test]
    fn watches_follow_visible_inner_sizes() {
        let mut state = ClientState::new(Tree::new(), "alice".into());
        state.terminals.insert(
            "alice".into(),
            ["one", "two"]
                .map(|pane| TerminalInfo {
                    pane: pane.into(),
                    name: "shell".into(),
                    state: "idle".into(),
                    cols: 80,
                    rows: 24,
                    last_typist: None,
                })
                .into(),
        );
        let (mut stream, mut peer) = UnixStream::pair().expect("streams must open");
        peer.set_read_timeout(Some(Duration::from_millis(50)))
            .expect("timeout must apply");
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).expect("backend must open");
        terminal
            .draw(|frame| render::draw(frame, &mut state))
            .expect("screen must draw");
        sync_watches(&mut stream, &mut state).expect("watches must send");
        for (index, area) in &state.box_areas {
            assert_eq!(
                codec::decode::<_, ClientMsg>(&mut peer).expect("watch must arrive"),
                ClientMsg::Watch {
                    user: "alice".into(),
                    pane: state.selected_terminals()[*index].pane.clone(),
                    cols: area.width - 2,
                    rows: area.height - 2,
                }
            );
        }
        sync_watches(&mut stream, &mut state).expect("unchanged watches must sync");
        let mut byte = [0];
        assert!(std::io::Read::read(&mut peer, &mut byte).is_err());
        state.open_focused();
        terminal
            .draw(|frame| render::draw(frame, &mut state))
            .expect("viewer must draw");
        sync_watches(&mut stream, &mut state).expect("viewer watch must send");
        assert_eq!(
            codec::decode::<_, ClientMsg>(&mut peer).expect("unwatch must arrive"),
            ClientMsg::Unwatch {
                user: "alice".into(),
                pane: "two".into()
            }
        );
        assert_viewer_watch(&mut peer, &state);
        terminal.backend_mut().resize(100, 30);
        terminal
            .draw(|frame| render::draw(frame, &mut state))
            .expect("resized viewer must draw");
        sync_watches(&mut stream, &mut state).expect("resized watch must send");
        assert_viewer_watch(&mut peer, &state);
        state.viewer = None;
        terminal
            .draw(|frame| render::draw(frame, &mut state))
            .expect("grid must draw");
        sync_watches(&mut stream, &mut state).expect("grid watches must send");
        for (index, area) in &state.box_areas {
            assert_eq!(
                codec::decode::<_, ClientMsg>(&mut peer).expect("grid watch must arrive"),
                ClientMsg::Watch {
                    user: "alice".into(),
                    pane: state.selected_terminals()[*index].pane.clone(),
                    cols: area.width - 2,
                    rows: area.height - 2,
                }
            );
        }
    }

    fn assert_viewer_watch(peer: &mut UnixStream, state: &ClientState) {
        let viewer = state.viewer.as_ref().expect("viewer must exist");
        assert_eq!(
            codec::decode::<_, ClientMsg>(peer).expect("viewer watch must arrive"),
            ClientMsg::Watch {
                user: viewer.user.clone(),
                pane: viewer.pane.clone(),
                cols: viewer.area.width - 2,
                rows: viewer.area.height - 2,
            }
        );
    }
}

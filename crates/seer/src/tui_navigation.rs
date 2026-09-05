use crate::{state::ClientState, tui::send};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use seer_core::proto::ClientMsg;
use seer_net::Stream;
use std::{cell::RefCell, io, io::Write};

thread_local! { static START_PERSON: RefCell<Option<String>> = const { RefCell::new(None) }; }
pub(crate) fn set_peek_person(person: Option<&str>) {
    START_PERSON.set(person.map(str::to_owned));
}
pub(crate) fn take_start_person() -> Option<String> {
    START_PERSON.take()
}

pub(crate) fn key(
    key: KeyEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<bool> {
    if std::mem::take(&mut state.discard_prefix) {
        return Ok(false);
    }
    if key.modifiers.intersects(
        KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SUPER
            | KeyModifiers::HYPER
            | KeyModifiers::META,
    ) {
        state.discard_prefix =
            key.code == KeyCode::Char('b') && key.modifiers == KeyModifiers::CONTROL;
        return Ok(false);
    }
    if state.close_prompt.is_some() {
        close_key(key, stream, state)?;
        return Ok(false);
    }
    if state.quit_prompt {
        match key.code {
            KeyCode::Enter | KeyCode::Char('q') => return Ok(true),
            KeyCode::Esc => state.quit_prompt = false,
            _ => {}
        }
        return Ok(false);
    }
    if state.searching {
        search(key, state);
        return Ok(false);
    }
    match key.code {
        KeyCode::Char(number @ '1'..='9') => state.select_tab(number as usize - '1' as usize),
        KeyCode::Char('x') => state.request_close(),
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Esc => state.quit_prompt = true,
        KeyCode::Char('j') | KeyCode::Down => move_person(state, true),
        KeyCode::Char('k') | KeyCode::Up => move_person(state, false),
        KeyCode::Char('h') | KeyCode::Left => move_box(state, false),
        KeyCode::Char('l') | KeyCode::Right => move_box(state, true),
        KeyCode::Enter => state.open_focused(),
        KeyCode::Char('p') => state.chrome.show_people = !state.chrome.show_people,
        KeyCode::Char('/') => state.searching = true,
        KeyCode::Char('n') => new_terminal(stream, state)?,
        KeyCode::Char('c') if state.people.len() == 1 => copy_invite(state)?,
        _ => {}
    }
    Ok(false)
}

pub(crate) fn close_key(
    key: KeyEvent,
    stream: &mut impl Stream,
    state: &mut ClientState,
) -> io::Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') => state.close_prompt = None,
        KeyCode::Enter | KeyCode::Char('y') => {
            if let Some(pane) = state.close_prompt.take()
                && let Some((workspace, tab)) = state.location(&pane)
            {
                send(
                    stream,
                    &ClientMsg::ClosePane {
                        workspace,
                        tab,
                        pane,
                    },
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn search(key: KeyEvent, state: &mut ClientState) {
    match key.code {
        KeyCode::Esc => {
            state.searching = false;
            state.search.clear();
        }
        KeyCode::Enter => state.searching = false,
        KeyCode::Backspace => {
            state.search.pop();
        }
        KeyCode::Char(character) => state.search.push(character),
        _ => {}
    }
    if let Some(index) = state.matches().first() {
        state.select_person(*index);
    }
    state.people_scroll = 0;
}

fn move_person(state: &mut ClientState, forward: bool) {
    state.chrome.grid_focus = false;
    let matches = state.matches();
    if matches.is_empty() {
        return;
    }
    let index = matches
        .iter()
        .position(|index| *index == state.selected)
        .unwrap_or(0);
    let index = step(index, matches.len(), forward);
    state.select_person(matches[index]);
    let height = state.people_areas.len().max(1);
    if index < state.people_scroll {
        state.people_scroll = index;
    }
    if index >= state.people_scroll + height {
        state.people_scroll = index + 1 - height;
    }
}

fn move_box(state: &mut ClientState, forward: bool) {
    state.chrome.grid_focus = true;
    let count = state.selected_terminals().len();
    if count == 0 {
        return;
    }
    state.focus = step(state.focus, count, forward);
    if !state
        .box_areas
        .iter()
        .any(|(index, _)| *index == state.focus)
    {
        state.grid_scroll = state.focus / state.grid_columns;
    }
}

pub(crate) fn step(index: usize, count: usize, forward: bool) -> usize {
    if forward {
        (index + 1) % count
    } else {
        (index + count - 1) % count
    }
}

fn new_terminal(stream: &mut impl Stream, state: &mut ClientState) -> io::Result<()> {
    let Some(workspace) = state.tree.workspaces.first() else {
        state.set_notice("Waiting for your terminals.");
        return Ok(());
    };
    state.pending_new = Some(
        state
            .tree
            .workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .flat_map(|t| &t.panes)
            .map(|p| p.id.clone())
            .collect(),
    );
    send(
        stream,
        &ClientMsg::CreateTab {
            workspace: workspace.id.clone(),
        },
    )?;
    Ok(())
}

pub(crate) fn invite(stream: &mut impl Stream, state: &mut ClientState) -> io::Result<()> {
    if !state.invite_pending {
        send(stream, &ClientMsg::Invite { hours: Some(24) })?;
        state.invite_pending = true;
    }
    Ok(())
}

fn copy_invite(state: &mut ClientState) -> io::Result<()> {
    let Some(invite) = &state.invite else {
        return Ok(());
    };
    let mut stdout = io::stdout().lock();
    write!(stdout, "\x1b]52;c;{}\x07", base64(invite.as_bytes()))?;
    stdout.flush()?;
    state.set_notice("Copy requested. Your terminal must permit clipboard access.");
    Ok(())
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::new();
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        text.push(char::from(ALPHABET[usize::from(first >> 2)]));
        text.push(char::from(
            ALPHABET[usize::from(((first & 3) << 4) | (second >> 4))],
        ));
        text.push(if chunk.len() > 1 {
            char::from(ALPHABET[usize::from(((second & 15) << 2) | (third >> 6))])
        } else {
            '='
        });
        text.push(if chunk.len() > 2 {
            char::from(ALPHABET[usize::from(third & 63)])
        } else {
            '='
        });
    }
    text
}

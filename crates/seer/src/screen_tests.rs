use crate::{render, state::ClientState, theme::Palette};
use ratatui::{Terminal, backend::TestBackend};
use seer_core::{
    Tree,
    proto::{Person, PersonState, TerminalInfo},
};

fn person(user: &str, name: &str) -> Person {
    Person {
        user_id: user.into(),
        name: name.into(),
        online: true,
        idle_secs: 0,
        attached_clients: 1,
        peekable: true,
        state: PersonState::Active,
        tabs: 0,
        foreground: String::new(),
    }
}

#[test]
fn main_screen_shows_people_terminals_and_input_permission() {
    let mut state = ClientState::new(Tree::new(), "alice".into());
    state.note_people(&[person("alice", "Alice"), person("bob", "Bob")]);
    state.selected = 1;
    state.terminals.insert(
        "bob".into(),
        ["claude", "codex", "shell"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| TerminalInfo {
                last_typist: None,
                pane: format!("p{i}"),
                name: name.into(),
                state: if name == "shell" { "idle" } else { "busy" }.into(),
                cols: 80,
                rows: 24,
            })
            .collect(),
    );
    let mut terminal = Terminal::new(TestBackend::new(130, 35)).expect("backend must open");
    terminal
        .draw(|frame| render::draw(frame, &mut state))
        .expect("screen must draw");
    let buffer = terminal.backend().buffer();
    let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    for expected in [
        "Alice",
        "Bob",
        "claude",
        "codex",
        "shell",
        "you may type: no",
    ] {
        assert!(text.contains(expected), "screen must show {expected}");
    }
    let palette = Palette::default();
    assert_eq!(buffer[(1, 3)].bg, palette.surface0);
    assert_eq!(buffer[(0, 1)].fg, palette.accent);
    assert_eq!(buffer[(0, 0)].bg, palette.panel_bg);
    assert_eq!(state.box_areas.len(), 3);
}

#[test]
fn first_run_shows_the_join_line() {
    let mut state = ClientState::new(Tree::new(), "alice".into());
    state.note_people(&[person("alice", "Alice")]);
    state.invite = Some("seer join SEER1-host-7321-invite".into());
    let mut terminal = Terminal::new(TestBackend::new(110, 30)).expect("backend must open");
    terminal
        .draw(|frame| render::draw(frame, &mut state))
        .expect("screen must draw");
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Nobody else is here yet."));
    assert!(text.contains("seer join SEER1-host-7321-invite"));
    assert!(text.contains("you"));
}

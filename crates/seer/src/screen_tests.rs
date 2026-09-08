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
                last_typist: (i == 0).then(|| "Carol".into()),
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
        "input: read only",
        "Carol is typing",
    ] {
        assert!(text.contains(expected), "screen must show {expected}");
    }
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
    assert!(text.contains("seer join"));
    assert!(text.contains("SEER1-host-7321-invite"));
    assert!(text.contains("you"));
    state
        .terminals
        .insert("alice".into(), vec![terminal_info("shell")]);
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
    assert!(!text.contains("Nobody else is here yet."));
    assert!(text.contains("shell idle"));
    assert!(text.contains("c copy"));
}

#[test]
fn backgrounds_preserve_the_host_terminal() {
    use ratatui::{layout::Position, style::Color};
    let mut state = ClientState::new(Tree::new(), "alice".into());
    state.note_people(&[person("alice", "Alice"), person("bob", "Bob")]);
    let mut terminal = Terminal::new(TestBackend::new(130, 35)).expect("backend must open");
    terminal
        .draw(|frame| render::draw(frame, &mut state))
        .expect("screen must draw");
    let buffer = terminal.backend().buffer();
    for y in 0..35 {
        for x in 0..130 {
            let selected = state.people_areas.iter().any(|(index, area)| {
                *index == state.selected && area.contains(Position::new(x, y))
            });
            assert_eq!(
                buffer[(x, y)].bg,
                if selected {
                    Palette::default().surface0
                } else {
                    Color::Reset
                }
            );
        }
    }
}

fn terminal_info(name: &str) -> TerminalInfo {
    TerminalInfo {
        last_typist: None,
        pane: name.into(),
        name: name.into(),
        state: "idle".into(),
        cols: 80,
        rows: 24,
    }
}

#[test]
fn tab_strip_shows_terminals_and_number_keys_select() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut state = ClientState::new(Tree::new(), "alice".into());
    state.terminals.insert(
        "alice".into(),
        ["claude", "codex", "shell"]
            .into_iter()
            .map(terminal_info)
            .collect(),
    );
    let mut terminal = Terminal::new(TestBackend::new(150, 40)).expect("backend must open");
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
    for label in ["claude idle x", "codex idle x", "shell idle x", " + "] {
        assert!(text.contains(label));
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let mut stream =
        std::net::TcpStream::connect(listener.local_addr().expect("address must exist"))
            .expect("client must connect");
    let _peer = listener.accept().expect("server must accept");
    crate::tui_navigation::key(
        KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
        &mut stream,
        &mut state,
    )
    .expect("key must work");
    terminal
        .draw(|frame| render::draw(frame, &mut state))
        .expect("screen must draw");
    let buffer = terminal.backend().buffer();
    let selected: String = buffer
        .content
        .iter()
        .filter(|cell| cell.bg == Palette::default().surface0)
        .map(|cell| cell.symbol())
        .collect();
    assert!(selected.contains("codex idle x"));
    assert!(!selected.contains("claude idle x"));
}

#[test]
fn viewer_keeps_people_and_tabs_visible() {
    let mut state = ClientState::new(Tree::new(), "alice".into());
    state.note_people(&[person("alice", "Alice"), person("bob", "Bob")]);
    state
        .terminals
        .insert("bob".into(), vec![terminal_info("codex")]);
    state
        .terminals
        .get_mut("bob")
        .expect("terminals must exist")[0]
        .last_typist = Some("Carol".into());
    state.select_person(1);
    state.open_focused();
    let mut terminal = Terminal::new(TestBackend::new(130, 35)).expect("backend must open");
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
    for label in [
        "people",
        "you",
        "Bob",
        "codex idle x",
        "Bob  codex  read only",
        "Carol is typing",
        "esc back",
        "tab next terminal",
    ] {
        assert!(text.contains(label), "screen must show {label}");
    }
    assert!(!text.contains("follow"));
}

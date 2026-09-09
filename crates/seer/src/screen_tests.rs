use crate::{
    panels::{self, Panel},
    render,
    state::ClientState,
    theme::Palette,
};
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
    panels::open(&mut state, Panel::People);
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
    let mut text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    panels::close(&mut state);
    text.push_str(&draw_text(&mut state, 130, 35).join("\n"));
    for expected in [
        "you",
        "Bob",
        "claude",
        "codex",
        "shell",
        "read only",
        "typing Carol",
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
    assert!(text.contains("Your terminal"));
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
    assert_eq!(
        state.box_areas[0].content,
        ratatui::layout::Rect::new(0, 0, 110, 30)
    );
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
            let selected = [state.chrome.chip_area, state.chrome.handle_area]
                .iter()
                .any(|area| area.contains(Position::new(x, y)));
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

fn draw_text(state: &mut ClientState, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend must open");
    terminal
        .draw(|frame| render::draw(frame, state))
        .expect("screen must draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn session_lists_terminals_and_keeps_actions_visible_on_short_screens() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut state = ClientState::new(Tree::new(), "alice".into());
    state.terminals.insert(
        "alice".into(),
        (0..20)
            .map(|i| terminal_info(&format!("shell{i}")))
            .collect(),
    );
    let (local, _peer) = std::os::unix::net::UnixStream::pair().expect("streams");
    let mut stream =
        crate::routes::Routes::new(seer_net::Socket::from(local), None, "alice".to_owned());
    panels::open(&mut state, Panel::Session);
    for _ in 0..22 {
        crate::input::command(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut stream,
            &mut state,
        )
        .expect("down");
    }
    for height in [8, 16, 35] {
        let text = draw_text(&mut state, 46, height).join("\n");
        assert!(
            text.contains("q quit"),
            "selected action must remain visible: {text}"
        );
        assert!(!state.chrome.rows.is_empty());
    }
    crate::input::command(
        KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
        &mut stream,
        &mut state,
    )
    .expect("select");
    assert_eq!(state.focus, 1);
}

#[test]
fn viewer_fills_the_screen_and_access_stays_visible() {
    let mut state = ClientState::new(Tree::new(), "alice".into());
    state.note_people(&[person("alice", "Alice"), person("bob", "Bob")]);
    state
        .terminals
        .insert("bob".into(), vec![terminal_info("codex")]);
    state.select_person(1);
    state.open_focused();
    for width in [9, 20, 36, 80, 130] {
        let text = draw_text(&mut state, width, 24).join("\n");
        assert!(
            text.contains("Read only"),
            "access must survive narrow widths: {text}"
        );
        assert_eq!(
            state.viewer.as_ref().expect("viewer").area,
            ratatui::layout::Rect::new(0, 0, width, 24)
        );
        assert!(!text.contains("people"));
        state.you_may_type_into.insert("bob".into());
        assert!(
            draw_text(&mut state, width, 24)
                .join("\n")
                .contains("Can type")
        );
        state.you_may_type_into.clear();
    }
    panels::open(&mut state, Panel::Session);
    let text = draw_text(&mut state, 80, 24).join("\n");
    for label in ["Bob", "Read only", "1 codex", "back"] {
        assert!(text.contains(label), "missing {label}");
    }
}

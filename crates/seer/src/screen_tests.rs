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
        host: user == "alice",
        state: PersonState::Active,
        tabs: 0,
        foreground: String::new(),
    }
}

#[test]
fn the_picker_shows_every_person_and_the_permission_for_the_viewed_one() {
    let mut state = ClientState::new(Tree::new(), "alice".into());
    state.note_people(&[person("alice", "Alice"), person("bob", "Bob")]);
    state.server = "127.0.0.1:7321".into();
    state.selected = 1;
    panels::open(&mut state, Panel::Picker);
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
    let text = draw_text(&mut state, 130, 35).join("\n");
    for expected in [
        "Seer Bob",
        "Alice",
        "host",
        "Permissions not granted for Bob",
        "127.0.0.1:7321",
    ] {
        assert!(
            text.contains(expected),
            "picker must show {expected}: {text}"
        );
    }

    panels::open(&mut state, Panel::Session);
    let text = draw_text(&mut state, 130, 35).join("\n");
    for expected in ["claude", "codex", "shell", "Read only", "typing Carol"] {
        assert!(text.contains(expected), "session must show {expected}");
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
    assert!(text.contains("Seer Alice"));
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
            let selected = state.chrome.chip_area.contains(Position::new(x, y));
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
            text.contains("Bob"),
            "the viewed name must survive narrow widths: {text}"
        );
        assert_eq!(
            state.viewer.as_ref().expect("viewer").area,
            ratatui::layout::Rect::new(0, 0, width, 24)
        );
        for absent in ["people", "Read only", "Can type"] {
            assert!(!text.contains(absent), "the control must not show {absent}");
        }
    }
    panels::open(&mut state, Panel::Session);
    let text = draw_text(&mut state, 80, 24).join("\n");
    for label in ["Bob", "Read only", "1 codex", "back"] {
        assert!(text.contains(label), "missing {label}");
    }
    state.you_may_type_into.insert("bob".into());
    assert!(
        draw_text(&mut state, 80, 24)
            .join("\n")
            .contains("Can type")
    );
}

#[test]
fn the_picker_takes_the_height_of_the_people_and_fills_five_row_columns() {
    for count in [1, 2, 4, 5, 6, 11] {
        let mut state = ClientState::new(Tree::new(), "alice".into());
        let mut people = vec![person("alice", "alice")];
        people.extend((1..count).map(|index| {
            let user = format!("u{index}");
            person(&user, &format!("name{index}"))
        }));
        state.note_people(&people);
        panels::open(&mut state, Panel::Picker);
        draw_text(&mut state, 100, 30);

        let rows = count.min(5);
        assert_eq!(
            state.chrome.panel_area.height,
            6 + rows as u16,
            "{count} people must take {rows} rows"
        );
        assert_eq!(state.people_areas.len(), count, "{count} people must show");
        let (top, left, width) = (
            state.people_areas[0].1.y,
            state.people_areas[0].1.x,
            state.people_areas[0].1.width,
        );
        for (index, rect) in &state.people_areas {
            assert_eq!(
                (rect.x, rect.y),
                (
                    left + (index / rows) as u16 * width,
                    top + (index % rows) as u16
                ),
                "person {index} of {count} must fill the column downwards"
            );
        }
    }
}

#[test]
fn the_picker_measures_wide_names_by_display_width() {
    let mut state = ClientState::new(Tree::new(), "alice".into());
    let wide_name = "\u{5f20}".repeat(10);
    state.note_people(&[person("alice", "alice"), person("bob", &wide_name)]);
    panels::open(&mut state, Panel::Picker);
    draw_text(&mut state, 100, 30);
    let wide = state
        .people_areas
        .iter()
        .find(|(index, _)| *index == 1)
        .expect("the wide name must show")
        .1;
    assert!(
        wide.width >= 22,
        "ten two cell names need twenty cells, not ten: {wide:?}"
    );
}

#[test]
fn application_colors_keep_the_terminal_palette() {
    use crate::terminal_cells::PaneCells;
    use ratatui::style::Color;
    use seer_core::{Cell, Color as CellColor};
    let cell = |fg: CellColor, bg: CellColor| Cell {
        character: 'x',
        fg,
        bg,
        bold: false,
        italic: false,
        underline: false,
        dim: false,
        inverse: false,
        hidden: false,
        strikeout: false,
    };
    let rows = vec![vec![
        cell(CellColor::Indexed(3), CellColor::Indexed(7)),
        cell(CellColor::Default, CellColor::Default),
        cell(
            CellColor::Rgb {
                red: 10,
                green: 20,
                blue: 30,
            },
            CellColor::Indexed(123),
        ),
    ]];
    let mut terminal = Terminal::new(TestBackend::new(3, 1)).expect("backend must open");
    terminal
        .draw(|frame| frame.render_widget(PaneCells::new(&rows), frame.area()))
        .expect("screen must draw");
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].fg, Color::Indexed(3));
    assert_eq!(buffer[(0, 0)].bg, Color::Indexed(7));
    assert_eq!(buffer[(1, 0)].fg, Color::Reset);
    assert_eq!(buffer[(1, 0)].bg, Color::Reset);
    assert_eq!(buffer[(2, 0)].fg, Color::Rgb(10, 20, 30));
    assert_eq!(buffer[(2, 0)].bg, Color::Indexed(123));
}

#[test]
fn a_remote_screen_keeps_row_zero_in_the_tile_and_open_view() {
    let mut state = ClientState::new(Tree::new(), "alice".into());
    state.note_people(&[person("alice", "Alice"), person("bob", "Bob")]);
    state.selected = 1;
    state
        .terminals
        .insert("bob".into(), vec![terminal_info("shell")]);
    let pane = state.selected_terminals()[0].pane.clone();
    let mut grid = seer_runtime::PaneGrid::new(80, 30);
    grid.feed(b"\x1b[1;1HTab one   Tab two\x1b[2;1HWorkspace\x1b[30;1HBottom row");
    state.frames.insert(("bob".into(), pane), grid.snapshot());
    let tile = draw_text(&mut state, 80, 24);
    assert!(
        tile[0].starts_with("Tab one   Tab two"),
        "row zero must stay visible: {tile:?}"
    );
    assert!(tile[1].starts_with("Workspace"));
    state.open_focused();
    for height in [24, 30, 34] {
        let opened = draw_text(&mut state, 80, height);
        assert!(opened[0].starts_with("Tab one   Tab two"));
        assert!(opened[1].starts_with("Workspace"));
        if height < 30 {
            assert!(!opened.iter().any(|row| row.contains("Bottom row")));
        } else {
            assert!(opened[29].starts_with("Bottom row"));
        }
    }
}

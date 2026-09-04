use crossterm::event::KeyCode;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::test_support::*;
use super::{draw, set_peek_person, set_view_only};
use crate::drawer::Drawer;

#[test]
fn enter_on_the_own_row_sends_no_peek() {
    let (mut client, mut server) = socket_pair();
    let mut state = state_with_pane();
    let mut drawer = Drawer::new("alice".into());
    set_view_only(false);
    drawer.toggle();
    apply_two_people(&mut client, &mut state, &mut drawer);

    press_key(&mut client, &mut state, &mut drawer, KeyCode::Enter);

    assert!(drawer.is_open());
    assert_no_message(&mut server);
}

#[test]
fn peek_banner_and_drawer_are_fixed_over_the_tree() {
    let mut terminal = Terminal::new(TestBackend::new(80, 5)).expect("terminal must start");
    let mut state = state_with_pane();
    let mut drawer = Drawer::new("carol".into());
    set_peek_person(Some("alice"));

    terminal
        .draw(|frame| draw(frame, &mut state, &drawer))
        .expect("frame must draw");

    let buffer = terminal.backend().buffer();
    let first_line: String = (0..23).map(|x| buffer[(x, 0)].symbol()).collect();
    let second_line: String = (0..79).map(|x| buffer[(x, 1)].symbol()).collect();
    let button: String = (68..80).map(|x| buffer[(x, 0)].symbol()).collect();
    assert_eq!(first_line, "PEEK: alice - READ ONLY");
    assert_eq!(second_line.trim_end(), "Workspace: alice/w1");
    assert_eq!(button, "[ People 0 ]");
    assert_eq!(buffer[(79, 2)].symbol(), "\u{2510}");

    drawer.toggle();
    terminal
        .draw(|frame| draw(frame, &mut state, &drawer))
        .expect("open drawer must draw");
    let title: String = (46..68)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
        .collect();
    assert!(title.starts_with("\u{250c}People"));
    let hint: String = (47..79)
        .map(|x| terminal.backend().buffer()[(x, 3)].symbol())
        .collect();
    assert_eq!(hint.trim_end(), "Enter peek  Esc close");
    set_peek_person(None);
}

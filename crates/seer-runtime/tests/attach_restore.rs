use seer_core::{InputEvent, KeyCode, KeyInput, Modifiers, TerminalInput};
use seer_runtime::PaneGrid;

const COLS: u16 = 20;
const ROWS: u16 = 5;

fn history_and_styles() -> Vec<u8> {
    let mut bytes = Vec::new();
    for line in 0..30 {
        bytes.extend(format!("\x1b[38;5;{line}mline {line}\x1b[0m\r\n").into_bytes());
    }
    bytes.extend(b"\x1b[1;3;4;7;38;2;10;20;30;48;5;200mstyled\x1b[0m \xe7\x95\x8c wide\r\n");
    bytes.extend(b"a long line that wraps past the edge of the screen\r\n");
    bytes.extend(b"\x1b[2;3H\x1b[31;42m\x1b7\x1b[0m");
    bytes.extend(b"\x1b[?1h\x1b[?2004h\x1b[?1000;1006h\x1b[5 q");
    bytes.extend(b"\x1b[5;1H\x1b[35mend of the last row!");
    bytes
}

fn restored(source: &mut PaneGrid) -> PaneGrid {
    let mut mirror = PaneGrid::mirror(COLS, ROWS);
    mirror.feed(&source.restore());
    mirror
}

fn scrolled(grid: &mut PaneGrid, lines: i32) -> seer_core::TerminalFrame {
    grid.handle_input(&TerminalInput::new(InputEvent::Scrollback { lines }))
        .expect("scrollback is valid input");
    grid.snapshot()
}

fn input(grid: &mut PaneGrid, event: InputEvent) -> Option<Vec<u8>> {
    grid.handle_input(&TerminalInput::new(event))
        .expect("input is valid")
}

fn assert_same(source: &mut PaneGrid, mirror: &mut PaneGrid) {
    assert_eq!(mirror.snapshot(), source.snapshot());
    assert_eq!(scrolled(mirror, 1_000), scrolled(source, 1_000));
    assert_eq!(scrolled(mirror, 7), scrolled(source, 7));
    assert_eq!(scrolled(mirror, -1_007), scrolled(source, -1_007));
}

#[test]
fn a_reopened_window_shows_the_screen_scrollback_and_modes() {
    let mut source = PaneGrid::new(COLS, ROWS);
    source.feed(&history_and_styles());
    let mut mirror = restored(&mut source);

    assert_same(&mut source, &mut mirror);
    let up = InputEvent::Key(KeyInput {
        code: KeyCode::Up,
        modifiers: Modifiers::default(),
    });
    assert_eq!(input(&mut mirror, up.clone()), input(&mut source, up));
    let paste = InputEvent::Paste("text".into());
    assert_eq!(input(&mut mirror, paste.clone()), input(&mut source, paste));

    let next = b"X\x1b8Y\x1b[?1049h";
    source.feed(next);
    mirror.feed(next);
    assert_same(&mut source, &mut mirror);
}

#[test]
fn a_reopened_window_shows_a_full_screen_app_and_the_shell_under_it() {
    let mut source = PaneGrid::new(COLS, ROWS);
    source.feed(&history_and_styles());
    source.feed(b"\x1b[?1049h\x1b[H\x1b[44mapp screen\x1b[3;4Hmiddle");
    let mut mirror = restored(&mut source);

    assert_eq!(mirror.snapshot(), source.snapshot());

    let leave = b"\x1b[?1049lback in the shell";
    source.feed(leave);
    mirror.feed(leave);
    assert_same(&mut source, &mut mirror);
}

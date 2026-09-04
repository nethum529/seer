use seer_core::proto::{ClientMsg, codec};
use seer_core::{
    Color, CursorShape, InputEvent, KeyCode, KeyInput, Modifiers, MouseButton, MouseInput,
    MouseKind, TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalInput,
};
use seer_runtime::PaneGrid;

#[test]
fn terminal_behavior_fixture() {
    let capabilities = TerminalCapabilities {
        protocol_version: TERMINAL_PROTOCOL_VERSION,
    };
    let message = ClientMsg::TerminalCapabilities { capabilities };
    let mut wire = Vec::new();
    codec::encode(&mut wire, &message).expect("capabilities must encode");
    assert_eq!(
        codec::decode::<_, ClientMsg>(&mut wire.as_slice()).expect("capabilities must decode"),
        message
    );

    let mut grid = PaneGrid::new(4, 2);
    assert!(grid.feed(b"\x1b[1;2;3;4;7;8;9;31;48;5;123mX\x1b[2;3H\x1b[5 q"));
    let frame = grid.snapshot();
    let styled = &frame.rows[0][0];
    assert_eq!(styled.fg, Color::Indexed(1));
    assert_eq!(styled.bg, Color::Indexed(123));
    assert!(styled.bold && styled.italic && styled.underline && styled.dim);
    assert!(styled.inverse && styled.hidden && styled.strikeout);
    assert_eq!(frame.cursor.row, 1);
    assert_eq!(frame.cursor.column, 2);
    assert_eq!(frame.cursor.shape, CursorShape::Beam);
    assert!(frame.cursor.visible);

    assert!(!grid.feed(b"\x1b[?2026h\x1b[2Jpending"));
    assert_eq!(grid.snapshot(), frame);
    assert!(grid.feed(b"\x1b[?2026l"));
    assert_ne!(grid.snapshot(), frame);

    let modified_five = TerminalInput::new(InputEvent::Key(KeyInput {
        code: KeyCode::Function(5),
        modifiers: Modifiers {
            control: true,
            ..Modifiers::default()
        },
    }));
    assert_eq!(
        grid.handle_input(&modified_five)
            .expect("key input must be accepted"),
        Some(b"\x1b[15;5~".to_vec())
    );

    grid.feed(b"\x1b[?1000;1006h");
    let click = TerminalInput::new(InputEvent::Mouse(MouseInput {
        kind: MouseKind::Down,
        button: Some(MouseButton::Left),
        column: 2,
        row: 1,
        modifiers: Modifiers::default(),
    }));
    assert_eq!(
        grid.handle_input(&click)
            .expect("mouse input must be accepted"),
        Some(b"\x1b[<0;3;2M".to_vec())
    );

    grid.feed(b"\x1b[?1000;1006l\x1b[?2004h");
    let paste = TerminalInput::new(InputEvent::Paste("one\ntwo".into()));
    assert_eq!(
        grid.handle_input(&paste)
            .expect("paste input must be accepted"),
        Some(b"\x1b[200~one\ntwo\x1b[201~".to_vec())
    );

    grid.feed(b"\x1b[?1049l\x1b[2Jone\r\ntwo\r\nthree\r\nfour");
    let scroll = TerminalInput::new(InputEvent::Scrollback { lines: 1 });
    assert_eq!(
        grid.handle_input(&scroll)
            .expect("scroll input must be accepted"),
        None
    );
}

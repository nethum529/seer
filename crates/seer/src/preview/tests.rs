use std::time::Duration;

use seer_core::proto::{ServerMsg, codec};
use seer_core::{Cell, Color, Cursor, PaneSize, TerminalFrame, TerminalModes, Tree};
use seer_net::{Socket, Stream};

use super::Preview;
use crate::tui::test_support::socket_pair;

#[test]
fn poll_applies_tree_and_cells_from_the_peeked_runtime() {
    let (client, mut server) = socket_pair();
    let mut tree = Tree::new();
    tree.create_workspace("main")
        .expect("workspace must be created");
    tree.create_tab("w1", "shell", PaneSize { cols: 80, rows: 24 })
        .expect("tab must be created");
    let row = vec![Cell {
        character: 'x',
        fg: Color::Default,
        bg: Color::Default,
        bold: false,
        italic: false,
        underline: false,
        dim: false,
        inverse: false,
        hidden: false,
        strikeout: false,
    }];
    codec::encode(&mut server, &ServerMsg::Tree { tree }).expect("tree must encode");
    codec::encode(
        &mut server,
        &ServerMsg::Cells {
            pane: "w1:p1".into(),
            frame: TerminalFrame {
                rows: vec![row.clone()],
                cursor: Cursor::default(),
                modes: TerminalModes::default(),
            },
        },
    )
    .expect("cells must encode");

    let client = Socket::from(client);
    client
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("read timeout must be set");
    let mut preview = Preview::with_stream(client);
    preview.poll().expect("preview must poll");

    assert_eq!(preview.rows(), [row].as_slice());
}

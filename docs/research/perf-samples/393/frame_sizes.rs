// Prints the codec frame size (4 byte length plus JSON body) of Seer
// messages. It is not part of the workspace. To run it, put it as
// src/main.rs in a new crate with this dependency, then cargo run --release:
//   seer-core = { path = "<repo>/crates/seer-core" }
// The output is deterministic. frame_sizes.csv is its output at revision
// 8714a51 of seer-core.
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{Cell, Color, Cursor, InputEvent, TerminalFrame, TerminalInput, TerminalModes};

fn cell(character: char) -> Cell {
    Cell {
        character,
        fg: Color::Default,
        bg: Color::Default,
        bold: false,
        italic: false,
        underline: false,
        dim: false,
        inverse: false,
        hidden: false,
        strikeout: false,
    }
}

fn frame(cols: usize, rows: usize, text_rows: usize) -> TerminalFrame {
    let rows = (0..rows)
        .map(|row| {
            (0..cols)
                .map(|col| {
                    if row < text_rows {
                        char::from(b'a' + ((row + col) % 26) as u8)
                    } else {
                        ' '
                    }
                })
                .map(cell)
                .collect()
        })
        .collect();
    TerminalFrame {
        rows,
        cursor: Cursor::default(),
        modes: TerminalModes::default(),
    }
}

fn client_size(message: &ClientMsg) -> usize {
    let mut out = Vec::new();
    codec::encode(&mut out, message).expect("encode");
    out.len()
}

fn server_size(message: &ServerMsg) -> usize {
    let mut out = Vec::new();
    codec::encode(&mut out, message).expect("encode");
    out.len()
}

fn main() {
    println!("message,cols,rows,text_rows,frame_bytes");
    let input = ClientMsg::TerminalInput {
        workspace: "w1".into(),
        tab: "t1".into(),
        pane: "p1".into(),
        input: TerminalInput::new(InputEvent::Text("a".into())),
    };
    println!("TerminalInput_text_a,0,0,0,{}", client_size(&input));
    for (cols, rows) in [(80, 24), (200, 50)] {
        for text_rows in [0, rows / 2, rows] {
            let message = ServerMsg::Cells {
                user: "alice".into(),
                pane: "p1".into(),
                frame: frame(cols, rows, text_rows),
            };
            println!("Cells,{cols},{rows},{text_rows},{}", server_size(&message));
        }
    }
}

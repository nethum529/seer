// Prints the codec frame size (4 byte length plus JSON body) of Seer
// messages for issue 393. Run: cargo run -p seer-core --example frame_sizes
// docs/research/perf-samples/393/frame_sizes.csv is its output at seer-core
// revision 8714a51.
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{Cell, Color, Cursor, InputEvent, TerminalFrame, TerminalInput, TerminalModes};
use serde::Serialize;
use std::io;

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

fn size<T: Serialize>(message: &T) -> io::Result<usize> {
    let mut out = Vec::new();
    codec::encode(&mut out, message)?;
    Ok(out.len())
}

fn main() -> io::Result<()> {
    println!("message,cols,rows,text_rows,frame_bytes");
    let input = ClientMsg::TerminalInput {
        workspace: "w1".into(),
        tab: "t1".into(),
        pane: "p1".into(),
        input: TerminalInput::new(InputEvent::Text("a".into())),
    };
    println!("TerminalInput_text_a,0,0,0,{}", size(&input)?);
    for (cols, rows) in [(80, 24), (200, 50)] {
        for text_rows in [0, rows / 2, rows] {
            let message = ServerMsg::Cells {
                user: "alice".into(),
                pane: "p1".into(),
                frame: frame(cols, rows, text_rows),
                seq: 0,
            };
            println!("Cells,{cols},{rows},{text_rows},{}", size(&message)?);
        }
    }
    Ok(())
}

use super::{BLANK, apply, diff};
use crate::{Cell, Color, Cursor, CursorShape, MouseTracking, TerminalFrame, TerminalModes};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).expect("fits")
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

fn cell(rng: &mut Rng) -> Cell {
    if rng.chance(30) {
        return BLANK;
    }
    let character = char::from(b' ' + u8::try_from(rng.below(95)).expect("fits"));
    let mut cell = Cell { character, ..BLANK };
    if rng.chance(20) {
        cell.fg = Color::Indexed(u8::try_from(rng.below(16)).expect("fits"));
        cell.bold = rng.chance(50);
        cell.inverse = rng.chance(20);
    }
    cell
}

fn row(rng: &mut Rng, cols: usize) -> Vec<Cell> {
    if rng.chance(15) {
        return vec![BLANK; cols];
    }
    (0..cols).map(|_| cell(rng)).collect()
}

fn cursor(rng: &mut Rng) -> Cursor {
    Cursor {
        row: u16::try_from(rng.below(30)).expect("fits"),
        column: u16::try_from(rng.below(40)).expect("fits"),
        shape: if rng.chance(50) {
            CursorShape::Block
        } else {
            CursorShape::Beam
        },
        blinking: rng.chance(50),
        visible: rng.chance(80),
    }
}

fn frame(rng: &mut Rng, cols: usize, rows: usize) -> TerminalFrame {
    TerminalFrame {
        rows: (0..rows).map(|_| row(rng, cols)).collect(),
        cursor: cursor(rng),
        modes: TerminalModes {
            mouse_tracking: if rng.chance(20) {
                MouseTracking::Click
            } else {
                MouseTracking::None
            },
            alt_screen: rng.chance(20),
        },
    }
}

fn edit(rng: &mut Rng, frame: &mut TerminalFrame, count: usize) {
    for _ in 0..count {
        let row = rng.below(frame.rows.len());
        let column = rng.below(frame.rows[row].len());
        frame.rows[row][column] = cell(rng);
    }
}

fn shifted(rng: &mut Rng, a: &TerminalFrame, shift: i32, edits: usize) -> (TerminalFrame, usize) {
    let rows = a.rows.len();
    let cols = a.rows[0].len();
    let source = |row: usize| {
        usize::try_from(i64::try_from(row).expect("fits") + i64::from(shift))
            .ok()
            .filter(|source| *source < rows)
    };
    let mut b = a.clone();
    b.rows = (0..rows)
        .map(|r| match source(r) {
            Some(source) => a.rows[source].clone(),
            None if rng.chance(50) => vec![BLANK; cols],
            None => row(rng, cols),
        })
        .collect();
    edit(rng, &mut b, edits);
    b.cursor = cursor(rng);
    let exposed: usize = (0..rows)
        .filter(|r| source(*r).is_none())
        .map(|r| b.rows[r].iter().filter(|cell| **cell != BLANK).count())
        .sum();
    (b, exposed + edits)
}

fn roundtrip(a: &TerminalFrame, b: &TerminalFrame) -> super::FrameDiff {
    let diff = diff(a, b).expect("same shape gives a diff");
    assert_eq!(apply(a, &diff).as_ref(), Ok(b));
    diff
}

fn check_cursor_only(rng: &mut Rng, a: &TerminalFrame) {
    let mut b = a.clone();
    b.cursor = cursor(rng);
    let diff = roundtrip(a, &b);
    assert!(diff.cells.is_empty());
    assert_eq!(diff.shift, 0);
}

fn check_shift(rng: &mut Rng, a: &TerminalFrame) {
    let rows = a.rows.len();
    let distance = i32::try_from(1 + rng.below(rows)).expect("fits");
    let shift = if rng.chance(50) { distance } else { -distance };
    let edits = rng.below(4);
    let (b, bound) = shifted(rng, a, shift, edits);
    let diff = roundtrip(a, &b);
    let kept_rows = rows - usize::try_from(distance).expect("fits");
    assert!(
        kept_rows < edits + 2 || diff.cells.len() <= bound,
        "shift {shift} with {edits} edits gave {} cells at shift {}, bound {bound}, {}x{}",
        diff.cells.len(),
        diff.shift,
        a.rows[0].len(),
        rows
    );
}

// A scroll where one column changes on every row, like a relative line
// number column in an editor.
fn check_shift_with_column(rng: &mut Rng, a: &TerminalFrame) {
    let rows = a.rows.len();
    let cols = a.rows[0].len();
    let distance = i32::try_from(1 + rng.below(rows)).expect("fits");
    let shift = if rng.chance(50) { distance } else { -distance };
    let (mut b, exposed) = shifted(rng, a, shift, 0);
    let column = rng.below(cols);
    for (row, cells) in b.rows.iter_mut().enumerate() {
        cells[column] = Cell {
            character: char::from(b'0' + u8::try_from(row % 10).expect("fits")),
            fg: Color::Indexed(3),
            ..BLANK
        };
    }
    let diff = roundtrip(a, &b);
    let kept_rows = rows - usize::try_from(distance).expect("fits");
    assert!(
        kept_rows < 2 || diff.cells.len() <= exposed + rows,
        "shift {shift} with a changed column gave {} cells at shift {}, bound {}, {cols}x{rows}",
        diff.cells.len(),
        diff.shift,
        exposed + rows
    );
}

fn check_edits(rng: &mut Rng, a: &TerminalFrame) {
    let mut b = a.clone();
    let count = 1 + rng.below(10);
    edit(rng, &mut b, count);
    roundtrip(a, &b);
}

fn check_other_shape(rng: &mut Rng, a: &TerminalFrame) {
    let rows = a.rows.len();
    let cols = a.rows[0].len();
    let b = if rng.chance(50) {
        frame(rng, cols + 1, rows)
    } else {
        frame(rng, cols, rows + 1)
    };
    assert!(diff(a, &b).is_none());
    let same = roundtrip(a, a);
    assert!(apply(&b, &same).is_err());
    let mut out_of_range = same;
    out_of_range
        .cells
        .push((u16::try_from(rows).expect("fits"), 0, BLANK));
    assert!(apply(a, &out_of_range).is_err());
    let mut ragged = a.clone();
    ragged.rows[0].push(BLANK);
    assert!(diff(&ragged, a).is_none());
}

#[test]
fn applying_the_diff_gives_the_new_frame() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..2000 {
        let cols = 1 + rng.below(40);
        let rows = 1 + rng.below(30);
        let a = frame(&mut rng, cols, rows);
        match rng.below(6) {
            0 => check_cursor_only(&mut rng, &a),
            1 => check_shift(&mut rng, &a),
            2 => check_shift_with_column(&mut rng, &a),
            3 => check_edits(&mut rng, &a),
            4 => {
                let b = frame(&mut rng, cols, rows);
                roundtrip(&a, &b);
            }
            _ => check_other_shape(&mut rng, &a),
        }
    }
}

use gpui::{Rgba, rgb};
use seer_core::{Cell, Color};

// The approved design uses gpui-component Default Dark.
pub(crate) const WINDOW: u32 = 0x0a0a0a;
pub(crate) const LINE: u32 = 0x262626;
pub(crate) const HOVER: u32 = 0x171717;
pub(crate) const PILL: u32 = 0x262626;
pub(crate) const CHIP_2: u32 = 0x404040;
pub(crate) const TEXT: u32 = 0xfafafa;
pub(crate) const TEXT_2: u32 = 0xd4d4d4;
pub(crate) const MUTED: u32 = 0xa3a3a3;
pub(crate) const FAINT: u32 = 0x737373;
pub(crate) const FAINTER: u32 = 0x525252;

const ANSI: [u32; 16] = [
    0x262626, 0xf87171, 0x4ade80, 0xfacc15, 0x60a5fa, 0xc084fc, 0x22d3ee, 0xd4d4d4, 0x737373,
    0xfca5a5, 0x86efac, 0xfde047, 0x93c5fd, 0xd8b4fe, 0x67e8f9, 0xfafafa,
];
const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

pub(crate) fn cursor() -> Rgba {
    rgb(TEXT_2)
}

// The text color and the background color of one cell. None is the window.
pub(crate) fn cell_colors(cell: &Cell) -> (Rgba, Option<Rgba>) {
    let mut fg = match cell.fg {
        Color::Default if cell.bold => rgb(TEXT),
        Color::Default => rgb(TEXT_2),
        color => terminal_color(color),
    };
    let mut bg = match cell.bg {
        Color::Default => None,
        color => Some(terminal_color(color)),
    };
    if cell.inverse {
        let swapped = bg.unwrap_or(rgb(WINDOW));
        bg = Some(fg);
        fg = swapped;
    }
    let behind = bg.unwrap_or(rgb(WINDOW));
    if cell.hidden {
        fg = behind;
    } else if cell.dim {
        fg = halfway(fg, behind);
    }
    (fg, bg)
}

fn terminal_color(color: Color) -> Rgba {
    match color {
        Color::Default => rgb(TEXT_2),
        Color::Indexed(index) => rgb(indexed(index)),
        Color::Rgb { red, green, blue } => rgb(pack(red, green, blue)),
    }
}

fn indexed(index: u8) -> u32 {
    match index {
        0..=15 => ANSI[usize::from(index)],
        16..=231 => {
            let cube = index - 16;
            pack(
                CUBE[usize::from(cube / 36)],
                CUBE[usize::from(cube / 6 % 6)],
                CUBE[usize::from(cube % 6)],
            )
        }
        _ => {
            let gray = 8 + (index - 232) * 10;
            pack(gray, gray, gray)
        }
    }
}

fn pack(red: u8, green: u8, blue: u8) -> u32 {
    u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue)
}

fn halfway(from: Rgba, to: Rgba) -> Rgba {
    Rgba {
        r: (from.r + to.r) / 2.,
        g: (from.g + to.g) / 2.,
        b: (from.b + to.b) / 2.,
        a: 1.,
    }
}

use gpui::{PathBuilder, px};

use super::{DeviceCell, box_arms::ARMS};

pub(super) fn draw(cell: &mut DeviceCell<'_>, ch: char) {
    match ch {
        '\u{2504}'..='\u{250b}' | '\u{254c}'..='\u{254f}' => dashed(cell, ch),
        '\u{256d}'..='\u{2570}' => rounded(cell, ch),
        '\u{2571}'..='\u{2573}' => diagonal(cell, ch),
        _ => lines(cell, ARMS[(ch as u32 - 0x2500) as usize]),
    }
}

fn bands(length: f32, stroke: f32, weight: u8) -> Vec<(f32, f32)> {
    let width = if weight == 2 { stroke * 2. } else { stroke };
    let lo = ((length - width) / 2.).floor();
    if weight == 3 {
        vec![(lo - stroke, lo), (lo + stroke, lo + stroke * 2.)]
    } else {
        vec![(lo, lo + width)]
    }
}

fn lines(cell: &mut DeviceCell<'_>, arms: [u8; 4]) {
    let [left, right, up, down] = arms;
    let h = left.max(right);
    let v = up.max(down);
    let t = cell.stroke();
    let xb = bands(cell.width, t, v);
    let yb = bands(cell.height, t, h);
    let (xlo, xhi) = (xb[0].0, xb[xb.len() - 1].1);
    let (ylo, yhi) = (yb[0].0, yb[yb.len() - 1].1);
    let double = h == 3 && v == 3;
    let (left_end, right_start, up_end, down_start) = if double {
        (xlo + t, xhi - t, ylo + t, yhi - t)
    } else {
        (xhi, xlo, yhi, ylo)
    };
    let ends = [left_end, right_start, up_end, down_start];
    for (side, weight) in arms.into_iter().enumerate() {
        if weight != 0 {
            arm(cell, side, weight, ends[side]);
        }
    }
    if double {
        double_join(cell, arms, xlo, xhi, ylo, yhi);
    }
}

fn arm(cell: &mut DeviceCell<'_>, side: usize, weight: u8, end: f32) {
    let length = if side < 2 { cell.height } else { cell.width };
    for (lo, hi) in bands(length, cell.stroke(), weight) {
        match side {
            0 => cell.rect(0., lo, end, hi),
            1 => cell.rect(end, lo, cell.width, hi),
            2 => cell.rect(lo, 0., hi, end),
            _ => cell.rect(lo, end, hi, cell.height),
        }
    }
}

fn double_join(cell: &mut DeviceCell<'_>, arms: [u8; 4], xlo: f32, xhi: f32, ylo: f32, yhi: f32) {
    let t = cell.stroke();
    if arms[0] == 0 {
        cell.rect(xlo, ylo, xlo + t, yhi);
    }
    if arms[1] == 0 {
        cell.rect(xhi - t, ylo, xhi, yhi);
    }
    if arms[2] == 0 {
        cell.rect(xlo, ylo, xhi, ylo + t);
    }
    if arms[3] == 0 {
        cell.rect(xlo, yhi - t, xhi, yhi);
    }
}

fn dashed(cell: &mut DeviceCell<'_>, ch: char) {
    let index = ch as u32;
    let (count, vertical) = if index < 0x254c {
        (if index < 0x2508 { 3 } else { 4 }, index % 4 >= 2)
    } else {
        (2, index >= 0x254e)
    };
    let t = cell.stroke() * if index % 2 == 1 { 2. } else { 1. };
    let length = if vertical { cell.height } else { cell.width };
    let cross = if vertical { cell.width } else { cell.height };
    let gap = (length / 8.).floor().max(1.);
    let dash = (length - gap * (count - 1) as f32) / count as f32;
    let lo = ((cross - t) / 2.).floor();
    for i in 0..count {
        let start = i as f32 * (dash + gap);
        if vertical {
            cell.rect(lo, start, lo + t, start + dash);
        } else {
            cell.rect(start, lo, start + dash, lo + t);
        }
    }
}

fn rounded(cell: &mut DeviceCell<'_>, ch: char) {
    let t = cell.stroke();
    let x = ((cell.width - t) / 2.).floor() + t / 2.;
    let y = ((cell.height - t) / 2.).floor() + t / 2.;
    let right = matches!(ch, '\u{256d}' | '\u{2570}');
    let down = matches!(ch, '\u{256d}' | '\u{256e}');
    let dx = if right { 1. } else { -1. };
    let dy = if down { 1. } else { -1. };
    let radius = (cell.width / 2. - t / 2.).min(cell.height / 2. - t / 2.);
    let mut path = PathBuilder::stroke(px(t / cell.scale));
    path.move_to(cell.point(if right { cell.width } else { 0. }, y));
    path.line_to(cell.point(x + dx * radius, y));
    path.curve_to(cell.point(x, y + dy * radius), cell.point(x, y));
    path.line_to(cell.point(x, if down { cell.height } else { 0. }));
    cell.path(path);
}

fn diagonal(cell: &mut DeviceCell<'_>, ch: char) {
    let mut path = PathBuilder::stroke(px(cell.stroke() / cell.scale));
    if ch != '\u{2572}' {
        path.move_to(cell.point(0., cell.height));
        path.line_to(cell.point(cell.width, 0.));
    }
    if ch != '\u{2571}' {
        path.move_to(cell.point(0., 0.));
        path.line_to(cell.point(cell.width, cell.height));
    }
    cell.path(path);
}

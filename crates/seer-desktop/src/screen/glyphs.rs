use gpui::{Bounds, PaintQuad, Path, PathBuilder, Pixels, Rgba, Window, fill, point, px, size};

#[path = "box_arms.rs"]
mod box_arms;
#[path = "boxes.rs"]
mod boxes;

#[derive(Clone, Default)]
pub(super) struct Glyphs {
    quads: Vec<PaintQuad>,
    paths: Vec<(Path<Pixels>, Rgba)>,
}

impl Glyphs {
    pub(super) fn append(&mut self, other: &Self) {
        self.quads.extend(other.quads.iter().cloned());
        self.paths.extend(other.paths.iter().cloned());
    }

    pub(super) fn paint(&self, window: &mut Window) {
        for quad in &self.quads {
            window.paint_quad(quad.clone());
        }
        for (path, color) in &self.paths {
            window.paint_path(path.clone(), *color);
        }
    }

    pub(super) fn add(&mut self, ch: char, bounds: Bounds<Pixels>, color: Rgba, scale: f32) {
        let left = (f32::from(bounds.left()) * scale).round();
        let top = (f32::from(bounds.top()) * scale).round();
        let mut cell = DeviceCell {
            glyphs: self,
            x: left,
            y: top,
            width: (f32::from(bounds.right()) * scale).round() - left,
            height: (f32::from(bounds.bottom()) * scale).round() - top,
            scale,
            stroke: (f32::from(bounds.size.width) * scale / 8.).round().max(1.),
            color,
        };
        if ch >= '\u{2580}' {
            cell.block(ch);
        } else {
            boxes::draw(&mut cell, ch);
        }
    }
}

pub(super) fn contains(ch: char) -> bool {
    ('\u{2500}'..='\u{259f}').contains(&ch)
}

struct DeviceCell<'a> {
    glyphs: &'a mut Glyphs,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    scale: f32,
    stroke: f32,
    color: Rgba,
}

impl DeviceCell<'_> {
    fn rect(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        let x1 = x1.round().clamp(0., self.width);
        let y1 = y1.round().clamp(0., self.height);
        let x2 = x2.round().clamp(0., self.width);
        let y2 = y2.round().clamp(0., self.height);
        if x2 > x1 && y2 > y1 {
            self.glyphs.quads.push(fill(
                Bounds::new(
                    self.point(x1, y1),
                    size(px((x2 - x1) / self.scale), px((y2 - y1) / self.scale)),
                ),
                self.color,
            ));
        }
    }

    fn point(&self, x: f32, y: f32) -> gpui::Point<Pixels> {
        point(px((self.x + x) / self.scale), px((self.y + y) / self.scale))
    }

    fn path(&mut self, builder: PathBuilder) {
        if let Ok(path) = builder.build() {
            self.glyphs.paths.push((path, self.color));
        }
    }

    fn block(&mut self, ch: char) {
        let (w, h) = (self.width, self.height);
        match ch {
            '\u{2580}' => self.rect(0., 0., w, h / 2.),
            '\u{2581}'..='\u{2588}' => {
                let eighths = (ch as u32 - 0x2580) as f32;
                self.rect(0., h - (h * eighths / 8.).round(), w, h);
            }
            '\u{2589}'..='\u{258f}' => {
                self.rect(0., 0., w * (0x2590 - ch as u32) as f32 / 8., h);
            }
            '\u{2590}' => self.rect(w / 2., 0., w, h),
            '\u{2591}'..='\u{2593}' => {
                self.color.a *= (ch as u32 - 0x2590) as f32 / 4.;
                self.rect(0., 0., w, h);
            }
            '\u{2594}' => self.rect(0., 0., w, h / 8.),
            '\u{2595}' => self.rect(w - (w / 8.).round(), 0., w, h),
            _ => self.quadrants(ch),
        }
    }

    fn quadrants(&mut self, ch: char) {
        let masks = [4, 8, 1, 13, 9, 7, 11, 2, 6, 14];
        let Some(mask) = masks.get((ch as u32 - 0x2596) as usize) else {
            return;
        };
        for quadrant in 0..4 {
            if mask & (1 << quadrant) == 0 {
                continue;
            }
            let x = (quadrant % 2) as f32 * self.width / 2.;
            let y = (quadrant / 2) as f32 * self.height / 2.;
            self.rect(x, y, x + self.width / 2., y + self.height / 2.);
        }
    }
}

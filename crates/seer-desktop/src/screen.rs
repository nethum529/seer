use gpui::{
    App, BorderStyle, Bounds, Element, ElementId, Entity, Font, FontStyle, FontWeight,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, PaintQuad, Pixels, Point, Rgba,
    ShapedLine, StrikethroughStyle, Style, TextRun, UnderlineStyle, Window, fill, font, outline,
    point, px, relative, size,
};
use seer_core::{Cell, TerminalFrame};

use crate::palette;
use crate::window::SeerWindow;

const FONT_SIZE: f32 = 12.5;
const LINE_HEIGHT: f32 = 1.55;
const MONO_FAMILIES: [&str; 8] = [
    "JetBrains Mono",
    "JetBrainsMono Nerd Font Mono",
    "Berkeley Mono",
    "SF Mono",
    "Menlo",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
];

pub(crate) fn terminal_font(cx: &App) -> Font {
    let installed = cx.text_system().all_font_names();
    let family = MONO_FAMILIES
        .into_iter()
        .find(|family| installed.iter().any(|name| name == family))
        .unwrap_or("monospace");
    font(family)
}

pub(crate) struct Screen {
    view: Entity<SeerWindow>,
    font: Font,
}

impl Screen {
    pub(crate) fn new(view: Entity<SeerWindow>, font: Font) -> Self {
        Self { view, font }
    }
}

#[derive(Default)]
pub(crate) struct Painted {
    quads: Vec<PaintQuad>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    cursor: Option<PaintQuad>,
    line_height: Pixels,
}

struct Grid {
    origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    cols: usize,
    rows: usize,
}

impl Grid {
    fn cell(&self, column: usize, row: usize, width: usize) -> Bounds<Pixels> {
        Bounds::new(
            point(
                self.origin.x + self.cell_width * column as f32,
                self.origin.y + self.line_height * row as f32,
            ),
            size(self.cell_width * width as f32, self.line_height),
        )
    }
}

#[derive(Default)]
struct Row {
    text: String,
    runs: Vec<TextRun>,
    style: Option<(Rgba, bool, bool, bool, bool)>,
    backgrounds: Vec<(usize, usize, Rgba)>,
}

impl IntoElement for Screen {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Screen {
    type RequestLayoutState = ();
    type PrepaintState = Painted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let grid = self.grid(bounds, window);
        let (cols, rows) = (to_u16(grid.cols), to_u16(grid.rows));
        self.view.update(cx, |view, cx| view.resize(cols, rows, cx));
        let Some(frame) = self.view.read(cx).frame() else {
            return Painted::default();
        };
        let mut painted = Painted {
            line_height: grid.line_height,
            ..Painted::default()
        };
        for (index, cells) in frame.rows.iter().take(grid.rows).enumerate() {
            let row = row(cells, grid.cols, &self.font);
            for (start, end, color) in &row.backgrounds {
                painted
                    .quads
                    .push(fill(grid.cell(*start, index, end - start), *color));
            }
            let line = window.text_system().shape_line(
                row.text.into(),
                px(FONT_SIZE),
                &row.runs,
                Some(grid.cell_width),
            );
            painted.lines.push((grid.cell(0, index, 0).origin, line));
        }
        painted.cursor = cursor(frame, &grid);
        painted
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        painted: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for quad in painted.quads.drain(..) {
            window.paint_quad(quad);
        }
        for (origin, line) in &painted.lines {
            line.paint(*origin, painted.line_height, window, cx).ok();
        }
        if let Some(cursor) = painted.cursor.take() {
            window.paint_quad(cursor);
        }
    }
}

impl Screen {
    fn grid(&self, bounds: Bounds<Pixels>, window: &Window) -> Grid {
        let font_size = px(FONT_SIZE);
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&self.font);
        let cell_width = text_system
            .advance(font_id, font_size, 'm')
            .map_or(font_size * 0.6, |advance| advance.width);
        let line_height = (font_size * LINE_HEIGHT).round();
        Grid {
            origin: bounds.origin,
            cell_width,
            line_height,
            cols: (bounds.size.width / cell_width).floor() as usize,
            rows: (bounds.size.height / line_height).floor() as usize,
        }
    }
}

fn row(cells: &[Cell], cols: usize, font: &Font) -> Row {
    let mut row = Row::default();
    for (column, cell) in cells.iter().take(cols).enumerate() {
        let (fg, bg) = palette::cell_colors(cell);
        let character = if cell.character.is_control() {
            ' '
        } else {
            cell.character
        };
        row.text.push(character);
        let style = (fg, cell.bold, cell.italic, cell.underline, cell.strikeout);
        match row.runs.last_mut() {
            Some(run) if row.style == Some(style) => run.len += character.len_utf8(),
            _ => row
                .runs
                .push(text_run(cell, fg, character.len_utf8(), font)),
        }
        row.style = Some(style);
        if let Some(bg) = bg {
            match row.backgrounds.last_mut() {
                Some((_, end, color)) if *end == column && *color == bg => *end += 1,
                _ => row.backgrounds.push((column, column + 1, bg)),
            }
        }
    }
    row
}

fn text_run(cell: &Cell, color: Rgba, len: usize, font: &Font) -> TextRun {
    let mut font = font.clone();
    if cell.bold {
        font.weight = FontWeight::BOLD;
    }
    if cell.italic {
        font.style = FontStyle::Italic;
    }
    TextRun {
        len,
        font,
        color: color.into(),
        background_color: None,
        underline: cell.underline.then(|| UnderlineStyle {
            color: Some(color.into()),
            thickness: px(1.),
            wavy: false,
        }),
        strikethrough: cell.strikeout.then(|| StrikethroughStyle {
            color: Some(color.into()),
            thickness: px(1.),
        }),
    }
}

// The window cannot type into the terminal, so the cursor has the hollow
// look of a terminal without focus, and it does not blink.
fn cursor(frame: &TerminalFrame, grid: &Grid) -> Option<PaintQuad> {
    let column = usize::from(frame.cursor.column);
    let row = usize::from(frame.cursor.row);
    (frame.cursor.visible && column < grid.cols && row < grid.rows).then(|| {
        outline(
            grid.cell(column, row, 1),
            palette::cursor(),
            BorderStyle::Solid,
        )
    })
}

fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

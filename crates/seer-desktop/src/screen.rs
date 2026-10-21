use gpui::{
    App, BorderStyle, Bounds, Element, ElementId, ElementInputHandler, Entity, Font, FontStyle,
    FontWeight, GlobalElementId, InspectorElementId, IntoElement, LayoutId, PaintQuad, Pixels,
    Point, Rgba, ShapedLine, StrikethroughStyle, Style, TextRun, UnderlineStyle, Window, fill,
    outline, point, px, relative, size,
};
use seer_core::{Cell, Cursor, CursorShape};

use crate::link::{Terminal, lock};
use crate::palette;
use crate::window::SeerWindow;

mod glyphs;

use glyphs::Glyphs;

pub(crate) struct Screen {
    view: Entity<SeerWindow>,
    font: Font,
    font_size: f32,
}

impl Screen {
    pub(crate) fn new(view: Entity<SeerWindow>, font: Font, font_size: f32) -> Self {
        Self {
            view,
            font,
            font_size,
        }
    }
}

#[derive(Default)]
pub(crate) struct Painted {
    quads: Vec<PaintQuad>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    glyphs: Glyphs,
    cursor: Option<PaintQuad>,
    line_height: Pixels,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Grid {
    pub(crate) origin: Point<Pixels>,
    pub(crate) cell_width: Pixels,
    pub(crate) line_height: Pixels,
    pub(crate) scale: f32,
    pub(crate) cols: usize,
    pub(crate) rows: usize,
}

impl Grid {
    pub(crate) fn contains(&self, position: Point<Pixels>) -> bool {
        Bounds::new(
            self.origin,
            size(
                self.cell_width * self.cols as f32,
                self.line_height * self.rows as f32,
            ),
        )
        .contains(&position)
    }
    pub(crate) fn cell(&self, column: usize, row: usize, width: usize) -> Bounds<Pixels> {
        Bounds::new(
            point(
                self.origin.x + self.cell_width * column as f32,
                self.origin.y + self.line_height * row as f32,
            ),
            size(self.cell_width * width as f32, self.line_height),
        )
    }

    pub(crate) fn hit(&self, position: Point<Pixels>) -> Option<(u16, u16, bool)> {
        if self.cols == 0 || self.rows == 0 {
            return None;
        }
        let x = ((position.x - self.origin.x) / self.cell_width).max(0.);
        let y = ((position.y - self.origin.y) / self.line_height).max(0.);
        let column = (x as usize).min(self.cols - 1);
        Some((
            to_u16(column),
            to_u16((y as usize).min(self.rows - 1)),
            x - column as f32 > 0.5,
        ))
    }
}

#[derive(Default)]
pub(crate) struct Rows {
    grid: Option<(Grid, usize)>,
    shaped: Vec<Shaped>,
}

#[derive(Clone, Default)]
struct Shaped {
    line: ShapedLine,
    backgrounds: Vec<(usize, usize, Rgba)>,
    glyphs: Glyphs,
}

#[derive(Default)]
struct Row {
    text: String,
    runs: Vec<TextRun>,
    style: Option<(Rgba, bool, bool, bool, bool)>,
    backgrounds: Vec<(usize, usize, Rgba)>,
    glyphs: Glyphs,
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
        self.view.update(cx, |view, cx| {
            let grid = view.sidebar_grid(grid);
            let (cols, rows) = (to_u16(grid.cols), to_u16(grid.rows));
            view.input.grid = Some(grid);
            view.resize(cols, rows, cx);
            let focused = view.input.focus.is_focused(window) && window.is_window_active();
            let Some((terminal, rows)) = view.screen() else {
                return Painted::default();
            };
            rows.paint(
                terminal,
                grid,
                (&self.font, self.font_size),
                focused,
                window,
                &palette::terminal(cx),
            )
        })
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        painted: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let colors = palette::terminal(cx);
        let view = self.view.read(cx);
        let focus = view.input.focus.clone();
        let preedit = view.input.composition.text.clone();
        let cursor = view.cursor_bounds();
        let selection = view
            .with_raw(|grid| grid.selection_spans())
            .unwrap_or_default();
        let geometry = view.input.grid;
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        let target = self.view.clone();
        window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
            if phase == gpui::DispatchPhase::Bubble {
                target.update(cx, |view, cx| view.mouse_move(event, cx));
            }
        });
        for quad in painted.quads.drain(..) {
            window.paint_quad(quad);
        }
        if let Some(grid) = geometry {
            for (row, start, end) in selection {
                window.paint_quad(fill(
                    grid.cell(start, row, end - start),
                    gpui::rgba(0xffffff30),
                ));
            }
        }
        for (origin, line) in &painted.lines {
            crate::terminal_font::paint(line, *origin, painted.line_height, window, cx);
        }
        painted.glyphs.paint(window);
        if let Some(cursor) = painted.cursor.take() {
            window.paint_quad(cursor);
        }
        if focus.is_focused(window) && window.is_window_active() {
            let cell = self
                .view
                .read(cx)
                .with_raw(|grid| {
                    let at = grid.cursor();
                    (at.visible && at.shape == CursorShape::Block)
                        .then(|| {
                            grid.row(usize::from(at.row))
                                .get(usize::from(at.column))
                                .filter(|cell| !cell.hidden)
                                .cloned()
                        })
                        .flatten()
                })
                .flatten();
            if let (Some(cell), Some(cursor)) = (cell, cursor) {
                if glyphs::contains(cell.character) {
                    let mut glyphs = Glyphs::default();
                    glyphs.add(
                        cell.character,
                        cursor,
                        colors.background(),
                        window.scale_factor(),
                    );
                    glyphs.paint(window);
                } else {
                    let text = cell.character.to_string();
                    let run = text_run(&cell, colors.background(), text.len(), &self.font);
                    let line = window.text_system().shape_line(
                        text.into(),
                        px(self.font_size),
                        &[run],
                        None,
                    );
                    crate::terminal_font::paint(
                        &line,
                        cursor.origin,
                        painted.line_height,
                        window,
                        cx,
                    );
                }
            }
        }
        if !preedit.is_empty()
            && let Some(cursor) = cursor
        {
            let len = preedit.len();
            let run = TextRun {
                len,
                font: self.font.clone(),
                color: colors.cursor().into(),
                background_color: None,
                underline: Some(UnderlineStyle {
                    color: None,
                    thickness: px(1.),
                    wavy: false,
                }),
                strikethrough: None,
            };
            let line =
                window
                    .text_system()
                    .shape_line(preedit.into(), px(self.font_size), &[run], None);
            window.paint_quad(fill(
                Bounds::new(cursor.origin, size(line.width, painted.line_height)),
                colors.background(),
            ));
            crate::terminal_font::paint(&line, cursor.origin, painted.line_height, window, cx);
        }
    }
}

impl Screen {
    fn grid(&self, bounds: Bounds<Pixels>, window: &Window) -> Grid {
        let font_size = px(self.font_size);
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&self.font);
        let cell_width = text_system
            .advance(font_id, font_size, 'm')
            .map_or(font_size * 0.6, |advance| advance.width);
        let scale = window.scale_factor();
        let line_height = (font_size * crate::terminal_font::LINE_RATIO * scale).ceil() / scale;
        Grid {
            origin: bounds.origin,
            cell_width,
            line_height,
            scale: window.scale_factor(),
            cols: (bounds.size.width / cell_width).floor() as usize,
            rows: (bounds.size.height / line_height).floor() as usize,
        }
    }
}

impl Rows {
    fn paint(
        &mut self,
        terminal: &Terminal,
        grid: Grid,
        (font, font_size): (&Font, f32),
        focused: bool,
        window: &mut Window,
        colors: &palette::TerminalColors,
    ) -> Painted {
        let (changed, at) = {
            let mut held = lock(terminal);
            let Some(terminal) = held.as_mut() else {
                return Painted::default();
            };
            let shown = (grid, grid.rows.min(terminal.rows()));
            let damage = terminal.take_damage();
            let changed: Vec<usize> = match damage {
                Some(damage) if self.grid == Some(shown) => {
                    damage.into_iter().filter(|row| *row < shown.1).collect()
                }
                _ => {
                    self.grid = Some(shown);
                    self.shaped = vec![Shaped::default(); shown.1];
                    (0..shown.1).collect()
                }
            };
            let changed: Vec<(usize, Vec<Cell>)> = changed
                .into_iter()
                .map(|index| (index, terminal.row(index)))
                .collect();
            (changed, terminal.cursor())
        };
        for (index, cells) in changed {
            self.shaped[index] = shape(&cells, &grid, index, font, font_size, window, colors);
        }
        let mut painted = Painted {
            line_height: grid.line_height,
            cursor: cursor(at, &grid, focused, colors),
            ..Painted::default()
        };
        for (index, shaped) in self.shaped.iter().enumerate() {
            painted.glyphs.append(&shaped.glyphs);
            for (start, end, color) in &shaped.backgrounds {
                painted
                    .quads
                    .push(fill(grid.cell(*start, index, end - start), *color));
            }
            painted
                .lines
                .push((grid.cell(0, index, 0).origin, shaped.line.clone()));
        }
        painted
    }
}

fn shape(
    cells: &[Cell],
    grid: &Grid,
    index: usize,
    font: &Font,
    font_size: f32,
    window: &Window,
    colors: &palette::TerminalColors,
) -> Shaped {
    let row = row(cells, grid, index, font, colors);
    let line = window.text_system().shape_line(
        row.text.into(),
        px(font_size),
        &row.runs,
        Some(grid.cell_width),
    );
    Shaped {
        line,
        backgrounds: row.backgrounds,
        glyphs: row.glyphs,
    }
}

fn row(
    cells: &[Cell],
    grid: &Grid,
    index: usize,
    font: &Font,
    colors: &palette::TerminalColors,
) -> Row {
    let mut row = Row::default();
    for (column, cell) in cells.iter().take(grid.cols).enumerate() {
        let (fg, bg) = colors.cell_colors(cell);
        let builtin = glyphs::contains(cell.character);
        if builtin && !cell.hidden {
            row.glyphs
                .add(cell.character, grid.cell(column, index, 1), fg, grid.scale);
        }
        let character = if builtin || cell.character.is_control() {
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
        font.weight = FontWeight::SEMIBOLD;
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

fn cursor(
    cursor: Cursor,
    grid: &Grid,
    focused: bool,
    colors: &palette::TerminalColors,
) -> Option<PaintQuad> {
    let column = usize::from(cursor.column);
    let row = usize::from(cursor.row);
    (cursor.visible && column < grid.cols && row < grid.rows).then(|| {
        let mut bounds = grid.cell(column, row, 1);
        if !focused || cursor.shape == CursorShape::HollowBlock {
            return outline(bounds, colors.cursor(), BorderStyle::Solid);
        }
        match cursor.shape {
            CursorShape::Beam => bounds.size.width = px(1.),
            CursorShape::Underline => {
                bounds.origin.y += bounds.size.height - px(1.);
                bounds.size.height = px(1.);
            }
            _ => {}
        }
        fill(bounds, colors.cursor())
    })
}

fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

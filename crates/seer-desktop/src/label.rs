use gpui::{
    AnyElement, App, Bounds, Element, ElementId, Global, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Point, ShapedLine, SharedString, TextAlign, TextLayout,
    TextStyleRefinement, Window, point, px,
};

pub(crate) fn text(text: impl Into<SharedString>) -> Label {
    Label {
        text: text.into(),
        multiline: false,
    }
}

pub(crate) fn tooltip(text: SharedString) -> Label {
    Label {
        text,
        multiline: true,
    }
}

pub(crate) struct Label {
    text: SharedString,
    multiline: bool,
}

impl IntoElement for Label {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn ink_height(cap_height: Pixels, scale: f32) -> Pixels {
    // 1/32 pixel coverage is below the 8/255 visible-ink contrast cutoff.
    (cap_height * scale - px(1. / 32.)).ceil()
}

pub(crate) fn center(bounds: Bounds<Pixels>, scale: f32) -> Pixels {
    let top = (bounds.top() * scale).round();
    let bottom = (bounds.bottom() * scale).round();
    (top + bottom) / 2.
}

pub(crate) fn baseline(
    bounds: Bounds<Pixels>,
    cap_height: Pixels,
    span: Pixels,
    scale: f32,
) -> Pixels {
    (center(bounds, scale) + ink_height(cap_height, scale) / 2. - span / 2.).round()
}

pub(crate) fn origin(
    line: &ShapedLine,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    baseline: Pixels,
    scale: f32,
) -> Point<Pixels> {
    let offset = (line_height - line.ascent - line.descent) / 2. + line.ascent;
    let mut origin = point(bounds.left(), baseline / scale - offset);
    // GPUI floors device origins; float division can land just below the target pixel.
    while (origin.y + offset) * scale < baseline {
        origin.y = px(f32::from(origin.y).next_up());
    }
    origin
}

pub(crate) fn cap_height(line: &ShapedLine, window: &Window) -> Pixels {
    let capital = line
        .text
        .chars()
        .find(|ch| ch.is_uppercase())
        .unwrap_or('H');
    capital_height(line, capital, window)
}

pub(crate) fn capital_height(line: &ShapedLine, capital: char, window: &Window) -> Pixels {
    line.runs.first().map_or(line.ascent - line.descent, |run| {
        let text = window.text_system();
        // GPUI 0.2.2 returns vertical advances instead of glyph bounds on Linux.
        if cfg!(target_os = "linux") {
            return text.cap_height(run.font_id, line.font_size);
        }
        text.typographic_bounds(run.font_id, line.font_size, capital)
            .map_or_else(
                |_| text.cap_height(run.font_id, line.font_size),
                |ink| ink.bottom(),
            )
    })
}

impl Element for Label {
    type RequestLayoutState = TextLayout;
    type PrepaintState = Option<Vec<(ShapedLine, Point<Pixels>)>>;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, TextLayout) {
        let refinement = self.multiline.then(|| {
            let style = window.text_style();
            let size = style.font_size.to_pixels(window.rem_size());
            let height = style.line_height.to_pixels(size.into(), window.rem_size());
            let scale = window.scale_factor();
            TextStyleRefinement {
                line_height: Some(((height * scale).round() / scale).into()),
                ..Default::default()
            }
        });
        window.with_text_style(refinement, |window| {
            self.text.request_layout(id, inspector_id, window, cx)
        })
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut TextLayout,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.text
            .prepaint(id, inspector_id, bounds, layout, window, cx);
        let wrapped = layout.wrapped_text();
        if !self.multiline && wrapped.contains('\n') {
            return None;
        }
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let lines: Vec<_> = wrapped
            .split('\n')
            .map(|text| {
                window.text_system().shape_line(
                    text.to_owned().into(),
                    font_size,
                    &[style.to_run(text.len())],
                    None,
                )
            })
            .collect();
        let first = lines.first()?;
        let scale = window.scale_factor();
        let pitch = (layout.line_height() * scale).round();
        let span = pitch * (lines.len() - 1) as f32;
        let baseline = baseline(
            bounds_for(bounds, cx),
            cap_height(first, window),
            span,
            scale,
        );
        Some(
            lines
                .into_iter()
                .enumerate()
                .map(|(index, line)| {
                    let mut origin = origin(
                        &line,
                        bounds,
                        layout.line_height(),
                        baseline + pitch * index as f32,
                        scale,
                    );
                    origin.x = match style.text_align {
                        TextAlign::Left => bounds.left(),
                        TextAlign::Center => {
                            (bounds.left() * 2. + bounds.size.width - line.width) / 2.
                        }
                        TextAlign::Right => bounds.right() - line.width,
                    };
                    (line, origin)
                })
                .collect(),
        )
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut TextLayout,
        lines: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(lines) = lines else {
            return self
                .text
                .paint(id, inspector_id, bounds, layout, &mut (), window, cx);
        };
        for (line, origin) in lines {
            if let Err(error) = line.paint(*origin, layout.line_height(), window, cx) {
                eprintln!("seer: could not draw label: {error}");
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct LabelBox(Option<Bounds<Pixels>>);
impl Global for LabelBox {}

pub(crate) fn bounds_for(fallback: Bounds<Pixels>, cx: &App) -> Bounds<Pixels> {
    cx.try_global::<LabelBox>()
        .and_then(|scope| scope.0)
        .unwrap_or(fallback)
}

pub(crate) fn centered(element: impl IntoElement) -> Centered {
    Centered(element.into_any_element())
}

pub(crate) struct Centered(AnyElement);

impl IntoElement for Centered {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Centered {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.0.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let previous = cx.try_global::<LabelBox>().copied().unwrap_or_default();
        cx.set_global(LabelBox(Some(bounds)));
        self.0.prepaint(window, cx);
        cx.set_global(previous);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.paint(window, cx);
    }
}

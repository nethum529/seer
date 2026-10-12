use gpui::{
    App, Bounds, Div, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Point, ShapedLine, SharedString, TextLayout, TextStyleRefinement, Window,
    point, px,
};

use std::{cell::Cell, rc::Rc};

pub(super) struct PixelTip {
    element: Div,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl PixelTip {
    pub(super) fn new(text: SharedString, build: impl FnOnce(PixelText) -> Div) -> Self {
        let bounds = Rc::new(Cell::new(Bounds::default()));
        Self {
            element: build(PixelText {
                text,
                tip_bounds: bounds.clone(),
            }),
            bounds,
        }
    }
}

impl IntoElement for PixelTip {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for PixelTip {
    type RequestLayoutState = <Div as Element>::RequestLayoutState;
    type PrepaintState = <Div as Element>::PrepaintState;

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
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.element.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.bounds.set(bounds);
        self.element
            .prepaint(id, inspector_id, bounds, layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.element
            .paint(id, inspector_id, bounds, layout, state, window, cx);
    }
}

pub(super) struct PixelText {
    text: SharedString,
    tip_bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl IntoElement for PixelText {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for PixelText {
    type RequestLayoutState = TextLayout;
    type PrepaintState = Vec<(ShapedLine, Point<Pixels>)>;

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
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style
            .line_height
            .to_pixels(font_size.into(), window.rem_size());
        let scale = window.scale_factor();
        let style = TextStyleRefinement {
            line_height: Some(((line_height * scale).round() / scale).into()),
            ..Default::default()
        };
        window.with_text_style(Some(style), |window| {
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
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let lines: Vec<_> = layout
            .wrapped_text()
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
        let Some(first) = lines.first() else {
            return Vec::new();
        };
        let cap_height = first
            .runs
            .first()
            .map_or(first.ascent - first.descent, |run| {
                window
                    .text_system()
                    .cap_height(run.font_id, first.font_size)
            });
        let scale = window.scale_factor();
        let pitch = (layout.line_height() * scale).round();
        let span = pitch * (lines.len() - 1) as f32;
        let tip = self.tip_bounds.get();
        let baseline =
            ((tip.top() + tip.size.height / 2. + cap_height / 2.) * scale - span / 2.).round();
        lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| {
                let baseline = baseline + pitch * index as f32;
                let offset = (layout.line_height() - line.ascent - line.descent) / 2. + line.ascent;
                let mut origin = point(bounds.left(), baseline / scale - offset);
                // GPUI floors device origins; float division can land just below the target pixel.
                while (origin.y + offset) * scale < baseline {
                    origin.y = px(f32::from(origin.y).next_up());
                }
                (line, origin)
            })
            .collect()
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        layout: &mut TextLayout,
        lines: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for (line, origin) in lines {
            if let Err(error) = line.paint(*origin, layout.line_height(), window, cx) {
                eprintln!("seer: could not draw tooltip text: {error}");
            }
        }
    }
}

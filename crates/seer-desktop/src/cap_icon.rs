use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window, point, px,
};

pub(crate) struct CapIcon(AnyElement);

pub(crate) fn cap_icon(icon: impl IntoElement) -> CapIcon {
    CapIcon(icon.into_any_element())
}

impl IntoElement for CapIcon {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CapIcon {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.0.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let style = window.text_style();
        let text = window.text_system();
        let font = text.resolve_font(&style.font());
        let font_size = style.font_size.to_pixels(window.rem_size());
        let ascent = text.ascent(font, font_size);
        let descent = text.descent(font, font_size).abs();
        let cap_height = text.cap_height(font, font_size);
        let center = bounds.center().y;
        let baseline = center + (ascent - descent) / 2.;
        let scale = window.scale_factor();
        // GPUI 0.2.2 rasterizes vertical glyph positions on whole device
        // pixels on Linux, and quarter device pixels on macOS.
        let variants = if cfg!(target_os = "macos") { 4. } else { 1. };
        let baseline = (baseline * scale * variants).floor() / variants;
        let cap_center = (baseline - cap_height * scale / 2.).round() / scale;
        window.with_element_offset(point(px(0.), cap_center - center), |window| {
            self.0.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.paint(window, cx);
    }
}

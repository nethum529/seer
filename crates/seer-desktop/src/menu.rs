use gpui::InteractiveElement;
use gpui::{Div, FontWeight, Stateful, Styled, div, point, px, rgb};

use crate::palette;

pub(crate) fn panel(id: &'static str, width: f32) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(width))
        .p(px(4.))
        .rounded(px(11.))
        .border_1()
        .border_color(rgb(palette::LINE_2))
        .bg(rgb(palette::RAISED))
        .text_size(px(13.))
        .line_height(px(18.))
        .shadow(shadow(16., 40.))
}

pub(crate) fn shadow(offset: f32, blur: f32) -> Vec<gpui::BoxShadow> {
    [(offset, blur), (2., 8.)]
        .map(|(offset, blur)| gpui::BoxShadow {
            color: gpui::rgba(palette::SHADOW).into(),
            offset: point(px(0.), px(offset)),
            blur_radius: px(blur),
            spread_radius: px(0.),
        })
        .into()
}

pub(crate) fn item(id: impl Into<gpui::ElementId>, height: f32) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(height))
        .px(px(8.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(10.))
        .text_color(rgb(palette::TEXT_2))
        .font_weight(FontWeight::MEDIUM)
}

pub(crate) fn separator() -> Div {
    div()
        .h(px(1.))
        .my(px(4.))
        .mx(px(-4.))
        .bg(rgb(palette::LINE_2))
}

use gpui::{Div, FontWeight, Stateful, Styled, div, point, px, rgb};
use gpui::{InteractiveElement, ParentElement, StatefulInteractiveElement};

use crate::{palette, sidebar, sidebar::Mode};
use gpui::prelude::FluentBuilder;
use gpui::{FocusHandle, MouseButton, Window};

pub(crate) fn panel(id: &'static str, width: f32, cx: &gpui::App) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(width))
        .p(px(4.))
        .rounded(px(11.))
        .border_1()
        .border_color(crate::appearance::fill(palette::LINE_2, cx))
        .bg(crate::appearance::fill(palette::RAISED, cx))
        .text_size(px(13.))
        .line_height(px(18.))
        .shadow(shadow(16., 40.))
}

pub(crate) fn sheet(id: &'static str, width: f32, cx: &gpui::App) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .relative()
        .w(px(width))
        .rounded(px(14.))
        .border_1()
        .border_color(crate::appearance::fill(palette::LINE_2, cx))
        .bg(crate::appearance::fill(palette::RAISED, cx))
        .shadow(shadow(24., 64.))
        .text_size(px(13.))
        .line_height(px(18.))
}

pub(crate) fn scrim(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .p(px(16.))
        .bg(gpui::rgba(palette::SCRIM))
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

pub(crate) fn switch_track(on: bool) -> Div {
    div()
        .w(px(32.))
        .h(px(18.))
        .flex_none()
        .rounded(px(9.))
        .p(px(2.))
        .bg(rgb(if on { palette::ACCENT } else { palette::LINE_2 }))
        .flex()
        .items_center()
        .when(on, |track| track.justify_end())
        .child(div().size(px(14.)).rounded(px(7.)).bg(rgb(if on {
            palette::ON_ACCENT
        } else {
            palette::TEXT
        })))
}

pub(crate) fn button(
    id: &'static str,
    text: &'static str,
    focus: &FocusHandle,
    accent: Option<u32>,
    mode: Mode,
    window: &Window,
) -> Stateful<Div> {
    let ring = mode == Mode::Keyboard && focus.is_focused(window);
    div()
        .id(id)
        .debug_selector(move || id.into())
        .relative()
        .track_focus(focus)
        .h(px(32.))
        .px(px(14.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .font_weight(FontWeight::MEDIUM)
        .bg(rgb(accent.unwrap_or(palette::CHIP)))
        .text_color(rgb(if accent.is_some() {
            palette::ON_ACCENT
        } else {
            palette::TEXT
        }))
        .when(ring, |button| {
            button.child(sidebar::outline().when(accent.is_some(), |outline| {
                outline
                    .top(px(-3.))
                    .bottom(px(-3.))
                    .left(px(-3.))
                    .right(px(-3.))
                    .rounded(px(11.))
            }))
        })
        .hover(|button| button.opacity(0.92))
        .active(|button| button.opacity(0.84))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(text)
}

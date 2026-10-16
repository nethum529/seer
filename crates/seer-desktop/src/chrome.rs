use gpui::{Div, ParentElement, Styled, Svg, WindowControlArea, div, px, rgb, svg};

use crate::cap_icon::cap_icon;
use crate::palette;
use seer_core::proto::AgentState;

pub(crate) fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

pub(crate) fn icon(name: &'static str, color: u32) -> Svg {
    svg()
        .path(name)
        .flex_none()
        .size(px(16.))
        .text_color(rgb(color))
}

pub(crate) fn status(state: Option<AgentState>) -> Div {
    div().flex_none().size(px(16.)).children(state.map(|state| {
        let (name, color) = match state {
            AgentState::Working => ("working", palette::MUTED),
            AgentState::Waiting => ("waiting", palette::WAITING),
            AgentState::Idle => ("idle", palette::FAINTER),
        };
        icon(name, color)
    }))
}

pub(crate) fn bar(cx: &gpui::App) -> Div {
    drag_area()
        .flex()
        .items_center()
        .bg(crate::appearance::fill(palette::SURFACE, cx))
        .border_b_1()
        .border_color(crate::appearance::fill(palette::LINE, cx))
}

pub(crate) fn drag_area() -> Div {
    use gpui::InteractiveElement;
    div()
        .flex_none()
        .h(px(44.))
        .window_control_area(WindowControlArea::Drag)
}

pub(crate) fn lights() -> Div {
    div().flex_none().w(px(56.)).h(px(44.))
}

#[cfg(target_os = "linux")]
pub(crate) fn window_controls(opacity: f32) -> Div {
    use gpui::{InteractiveElement, MouseButton};
    lights()
        .opacity(opacity)
        .flex()
        .items_center()
        .gap(px(8.))
        .children(
            [0xff5f57, 0xfebc2e, 0x28c840]
                .into_iter()
                .enumerate()
                .map(|(index, color)| {
                    div()
                        .size(px(12.))
                        .rounded_full()
                        .bg(rgb(color))
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.stop_propagation();
                            match index {
                                0 => window.remove_window(),
                                1 => window.minimize_window(),
                                _ => window.zoom_window(),
                            }
                        })
                }),
        )
}

pub(crate) fn room(room: &str, narrow: bool, compact: bool, up: bool) -> Div {
    use gpui::prelude::FluentBuilder;
    div()
        .h(px(32.))
        .min_w_0()
        .pl(px(if narrow { 8. } else { 10. }))
        .pr(px(8.))
        .flex()
        .items_center()
        .gap(px(if narrow { 8. } else { 10. }))
        .text_color(rgb(palette::MUTED))
        .child(cap_icon(
            div()
                .size(px(16.))
                .when(narrow, |slot| slot.w(px(8.)))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(room_dot(up)),
        ))
        .when(!compact, |row| {
            row.child(crate::sidebar::one_line(div()).child(crate::label::text(room.to_owned())))
        })
        .child(cap_icon(icon("selector", palette::FAINT)))
}

pub(crate) fn settings() -> Div {
    div()
        .size(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(cap_icon(icon("gear", palette::FAINT)))
}

pub(crate) fn footer(
    room: impl gpui::IntoElement,
    settings: impl gpui::IntoElement,
    cx: &gpui::App,
) -> Div {
    div()
        .flex_none()
        .h(px(44.))
        .mx(px(-8.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .border_t_1()
        .border_color(crate::appearance::fill(palette::LINE, cx))
        .text_color(rgb(palette::MUTED))
        .child(div().flex_1().min_w_0().flex().items_center().child(room))
        .child(settings)
}

pub(crate) fn room_dot(up: bool) -> Div {
    use gpui::prelude::FluentBuilder;
    div()
        .size(px(if up { 6. } else { 8. }))
        .rounded_full()
        .when(up, |dot| dot.bg(rgb(palette::ONLINE)))
        .when(!up, |dot| {
            dot.border(px(1.5)).border_color(rgb(palette::FAINTER))
        })
}

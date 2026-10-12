use gpui::{Div, ParentElement, Styled, Svg, WindowControlArea, div, px, rgb, svg};

use crate::cap_icon::cap_icon;
use crate::palette;

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

pub(crate) fn status(state: &str) -> Svg {
    let (name, color) = match state {
        "busy" | "working" => ("working", palette::MUTED),
        "waiting" => ("waiting", palette::WAITING),
        _ => ("idle", palette::FAINTER),
    };
    // The runtime cannot distinguish working from waiting yet (T-481).
    icon(name, color).invisible()
}

pub(crate) fn bar() -> Div {
    use gpui::InteractiveElement;
    div()
        .flex_none()
        .h(px(44.))
        .flex()
        .items_center()
        .bg(rgb(palette::SURFACE))
        .border_b_1()
        .border_color(rgb(palette::LINE))
        .window_control_area(WindowControlArea::Drag)
}

pub(crate) fn lights() -> Div {
    let lights = div().flex_none().w(px(56.)).h(px(44.));
    #[cfg(target_os = "linux")]
    {
        use gpui::{InteractiveElement, MouseButton};
        lights.flex().items_center().gap(px(8.)).children(
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
    #[cfg(target_os = "macos")]
    lights
}

pub(crate) fn room(room: &str, narrow: bool, compact: bool) -> Div {
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
                .child(
                    div()
                        .size(px(8.))
                        .rounded_full()
                        .border(px(1.5))
                        .border_color(rgb(palette::FAINTER)),
                ),
        ))
        .when(!compact, |row| {
            row.child(crate::sidebar::one_line(div()).child(room.to_owned()))
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

pub(crate) fn footer(name: &str) -> Div {
    div()
        .flex_none()
        .h(px(44.))
        .mx(px(-8.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .border_t_1()
        .border_color(rgb(palette::LINE))
        .text_color(rgb(palette::MUTED))
        .child(room(name, false, false).flex_1())
        .child(settings())
}

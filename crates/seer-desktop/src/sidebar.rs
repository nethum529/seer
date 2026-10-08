use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Div, FontWeight, InteractiveElement, MouseButton, ParentElement, SharedString,
    Stateful, StatefulInteractiveElement, Styled, div, px, relative, rgb,
};
use seer_core::proto::TerminalInfo;

use crate::palette;
use crate::window::SeerWindow;

const WIDTH: f32 = 248.;

pub(crate) fn sidebar(
    name: &SharedString,
    terminals: &[TerminalInfo],
    shown: Option<&str>,
    cx: &mut Context<SeerWindow>,
) -> Div {
    div()
        .flex_none()
        .w(px(WIDTH))
        .h_full()
        .flex()
        .flex_col()
        .px(px(10.))
        .bg(rgb(palette::WINDOW))
        .border_r_1()
        .border_color(rgb(palette::LINE))
        .child(
            who(name, 22., 11.)
                .w_full()
                .h(px(40.))
                .mt(px(2.))
                .pl(px(8.))
                .pr(px(10.)),
        )
        .child(
            div()
                .id("tabs")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .mt(px(10.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(terminals.iter().map(|terminal| tab(terminal, shown, cx))),
        )
}

pub(crate) fn who(name: &SharedString, avatar: f32, initial: f32) -> Div {
    let letter: SharedString = name
        .chars()
        .next()
        .map(|first| first.to_uppercase().collect::<String>().into())
        .unwrap_or_default();
    div()
        .flex()
        .items_center()
        .gap(px(9.))
        .rounded(px(8.))
        .child(
            div()
                .flex_none()
                .size(px(avatar))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(palette::CHIP_2))
                .text_size(px(initial))
                .line_height(relative(1.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(letter),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(name.clone()),
        )
}

pub(crate) fn strip(
    terminals: &[TerminalInfo],
    shown: Option<&str>,
    cx: &mut Context<SeerWindow>,
) -> Stateful<Div> {
    div()
        .id("strip")
        .flex_none()
        .flex()
        .gap(px(4.))
        .py(px(6.))
        .px(px(8.))
        .overflow_x_scroll()
        .bg(rgb(palette::WINDOW))
        .border_b_1()
        .border_color(rgb(palette::LINE))
        .children(terminals.iter().map(|terminal| {
            let on = shown == Some(terminal.pane.as_str());
            selectable(terminal, cx)
                .flex_none()
                .h(px(32.))
                .px(px(12.))
                .gap(px(7.))
                .whitespace_nowrap()
                .text_color(rgb(if on { palette::TEXT } else { palette::TEXT_2 }))
                .when(on, |tab| tab.bg(rgb(palette::PILL)))
                .child(dot())
                .child(terminal.name.clone())
        }))
}

fn tab(terminal: &TerminalInfo, shown: Option<&str>, cx: &mut Context<SeerWindow>) -> Div {
    let row = selectable(terminal, cx)
        .w_full()
        .pt(px(7.))
        .pr(px(10.))
        .pb(px(8.))
        .pl(px(12.))
        .gap(px(10.))
        .child(dot())
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .child(terminal.name.clone()),
        );
    if shown == Some(terminal.pane.as_str()) {
        row.bg(rgb(palette::PILL))
    } else {
        row.hover(|style| style.bg(rgb(palette::HOVER)))
    }
}

fn selectable(terminal: &TerminalInfo, cx: &mut Context<SeerWindow>) -> Div {
    let pane = terminal.pane.clone();
    div().flex().items_center().rounded(px(8.)).on_mouse_down(
        MouseButton::Left,
        cx.listener(move |view, _, _, cx| view.select(&pane, cx)),
    )
}

// The design draws a ring for a terminal whose agent state is not known.
fn dot() -> Div {
    div()
        .flex_none()
        .size(px(7.))
        .rounded_full()
        .border_1()
        .border_color(rgb(palette::FAINTER))
}

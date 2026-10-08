use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Div, FocusHandle, FontWeight, InteractiveElement, KeyDownEvent, MouseButton,
    ParentElement, Pixels, ScrollHandle, SharedString, Stateful, StatefulInteractiveElement,
    Styled, Window, div, point, px, relative, rgb,
};
use seer_core::proto::TerminalInfo;

use crate::palette;
use crate::tip;
use crate::window::SeerWindow;

const WIDTH: f32 = 248.;
const STRIP_PAD: f32 = 8.;

pub(crate) struct Keys {
    pub(crate) tabs: FocusHandle,
    pub(crate) who: FocusHandle,
    pub(crate) scroll: ScrollHandle,
    pub(crate) mode: Mode,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Pointer,
    Keyboard,
    Quiet,
}

impl Keys {
    pub(crate) fn ring(&self, focus: &FocusHandle, window: &Window) -> bool {
        self.mode == Mode::Keyboard && focus.is_focused(window)
    }

    pub(crate) fn hover(&self) -> bool {
        self.mode != Mode::Quiet
    }

    // gpui scroll_to_item puts the item flush on the list edge and ignores
    // the list padding.
    pub(crate) fn reveal(&self, at: usize) {
        let Some(item) = self.scroll.bounds_for_item(at) else {
            return;
        };
        let view = self.scroll.bounds();
        let max = self.scroll.max_offset();
        let offset = self.scroll.offset();
        let pad = px(STRIP_PAD);
        let x = fit(
            item.left() - pad,
            item.right() + pad,
            view.left(),
            view.right(),
            offset.x,
        );
        let y = fit(
            item.top(),
            item.bottom(),
            view.top(),
            view.bottom(),
            offset.y,
        );
        self.scroll.set_offset(point(
            x.clamp(-max.width, px(0.)),
            y.clamp(-max.height, px(0.)),
        ));
    }
}

fn fit(start: Pixels, end: Pixels, view_start: Pixels, view_end: Pixels, offset: Pixels) -> Pixels {
    if start + offset < view_start {
        view_start - start
    } else if end + offset > view_end {
        view_end - end
    } else {
        offset
    }
}

pub(crate) fn sidebar(
    who: Stateful<Div>,
    terminals: &[TerminalInfo],
    shown: Option<&str>,
    keys: &Keys,
    window: &Window,
    cx: &mut Context<SeerWindow>,
) -> Div {
    let ring = keys.ring(&keys.tabs, window);
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
        .child(who.w_full().h(px(40.)).mt(px(2.)).pl(px(8.)).pr(px(10.)))
        .child(
            list(div().id("tabs"), keys, cx)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .mt(px(10.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(
                    terminals
                        .iter()
                        .map(|terminal| tab(terminal, shown, ring, cx)),
                ),
        )
}

pub(crate) fn who(
    name: &SharedString,
    avatar: f32,
    initial: f32,
    font: &SharedString,
    keys: &Keys,
    window: &Window,
) -> Stateful<Div> {
    let letter: SharedString = name
        .chars()
        .next()
        .map(|first| first.to_uppercase().collect::<String>().into())
        .unwrap_or_default();
    let ring = keys.ring(&keys.who, window);
    tip::on_hover(
        div().id("who"),
        name.clone(),
        font.clone(),
        keys.hover() && !ring,
    )
    .track_focus(&keys.who)
    .relative()
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
            // gpui draws this letter 1 px higher than the browser does in
            // the design, measured by ink rows at both avatar sizes.
            .child(div().relative().top(px(1.)).child(letter)),
    )
    .child(
        one_line(div().flex_1())
            .font_weight(FontWeight::SEMIBOLD)
            .child(name.clone()),
    )
    .when(ring, |who| {
        who.child(outline())
            .child(tip::below(name.clone(), font.clone()))
    })
}

// gpui 0.2.2 keeps the first measure of a text that does not wrap. That
// measure has no width, so truncate() never draws the ellipsis. A text that
// wraps is measured again for each width, and line_clamp keeps one line.
pub(crate) fn one_line(text: Div) -> Div {
    text.min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        .line_clamp(1)
}

pub(crate) fn strip(
    terminals: &[TerminalInfo],
    shown: Option<&str>,
    keys: &Keys,
    window: &Window,
    cx: &mut Context<SeerWindow>,
) -> Stateful<Div> {
    let ring = keys.ring(&keys.tabs, window);
    list(div().id("strip"), keys, cx)
        .flex_none()
        .flex()
        .gap(px(4.))
        .py(px(6.))
        .px(px(STRIP_PAD))
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
                .when(on && ring, |tab| tab.child(outline()))
                .child(dot())
                .child(terminal.name.clone())
        }))
}

// The tab list is one tab stop. The arrow keys move the selection, as in the
// WAI-ARIA tabs pattern with automatic activation.
pub(crate) fn step(key: &str, count: usize, now: Option<usize>) -> Option<usize> {
    let last = count.checked_sub(1)?;
    match key {
        "up" | "left" => Some(now.map_or(last, |at| at.checked_sub(1).unwrap_or(last))),
        "down" | "right" => Some(now.map_or(0, |at| if at == last { 0 } else { at + 1 })),
        "home" => Some(0),
        "end" => Some(last),
        "enter" | "space" => Some(now.unwrap_or(0)),
        _ => None,
    }
}

fn list(list: Stateful<Div>, keys: &Keys, cx: &mut Context<SeerWindow>) -> Stateful<Div> {
    list.track_focus(&keys.tabs)
        .track_scroll(&keys.scroll)
        .on_key_down(
            cx.listener(|view, event: &KeyDownEvent, _, cx| view.key(&event.keystroke, cx)),
        )
}

fn tab(
    terminal: &TerminalInfo,
    shown: Option<&str>,
    ring: bool,
    cx: &mut Context<SeerWindow>,
) -> Div {
    let row = selectable(terminal, cx)
        .w_full()
        .pt(px(7.))
        .pr(px(10.))
        .pb(px(8.))
        .pl(px(12.))
        .gap(px(10.))
        .child(dot())
        .child(
            one_line(div().flex_1())
                .font_weight(FontWeight::MEDIUM)
                .child(terminal.name.clone()),
        );
    if shown == Some(terminal.pane.as_str()) {
        row.bg(rgb(palette::PILL))
            .when(ring, |row| row.child(outline()))
    } else {
        row.hover(|style| style.bg(rgb(palette::HOVER)))
    }
}

fn selectable(terminal: &TerminalInfo, cx: &mut Context<SeerWindow>) -> Div {
    let pane = terminal.pane.clone();
    div()
        .relative()
        .flex()
        .items_center()
        .rounded(px(8.))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, window, cx| view.press(&pane, window, cx)),
        )
}

// The design draws the focus ring as a 1 px outline with a -1 px offset, so
// the ring sits on the inner edge of the tab.
fn outline() -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(palette::ACCENT))
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

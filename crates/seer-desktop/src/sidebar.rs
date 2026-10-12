use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Div, FocusHandle, Font, FontFeatures, FontWeight, InteractiveElement, KeyDownEvent,
    MouseButton, ParentElement, Pixels, ScrollHandle, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, div, point, px, relative, rgb,
};
use seer_core::proto::TerminalInfo;

use crate::cap_icon::cap_icon;
use crate::chrome;
use crate::palette;
use crate::tab_time;
use crate::tip;
use crate::window::SeerWindow;
use crate::window::rename::{self, Surface};

const WIDTH: f32 = 248.;
const STRIP_PAD: f32 = 8.;

pub(crate) struct Keys {
    pub(crate) tabs: FocusHandle,
    pub(crate) who: FocusHandle,
    pub(crate) new_terminal: FocusHandle,
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
    labels: rename::Labels<'_>,
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
        .px(px(8.))
        .bg(rgb(palette::SURFACE))
        .border_r_1()
        .border_color(rgb(palette::LINE))
        .child(
            div()
                .flex_none()
                .h(px(44.))
                .pl(px(12.))
                .child(chrome::lights()),
        )
        .child(who.w_full().h(px(36.)).pl(px(7.)).pr(px(8.)))
        .child(
            list(div().id("tabs"), keys, cx)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .mt(px(12.))
                .pb(px(8.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(
                    terminals
                        .iter()
                        .map(|terminal| tab(terminal, shown, labels, ring, cx)),
                )
                .child(new_terminal(keys, false, window, cx)),
        )
}

pub(crate) fn new_terminal(
    keys: &Keys,
    compact: bool,
    window: &Window,
    cx: &mut Context<SeerWindow>,
) -> Stateful<Div> {
    let label: SharedString = "New terminal".into();
    let ring = keys.ring(&keys.new_terminal, window);
    tip::on_hover(
        div().id("new-terminal"),
        label.clone(),
        "Inter".into(),
        keys.hover() && !ring,
    )
    .track_focus(&keys.new_terminal)
    .group("new-terminal")
    .relative()
    .flex_none()
    .w_full()
    .h(px(32.))
    .mt(px(4.))
    .px(px(10.))
    .rounded(px(8.))
    .flex()
    .items_center()
    .gap(px(10.))
    .font_weight(FontWeight::MEDIUM)
    .text_color(rgb(palette::MUTED))
    .hover(|style| style.bg(rgb(palette::HOVER)).text_color(rgb(palette::TEXT)))
    .on_click(cx.listener(|view, _, _, cx| view.new_terminal(cx)))
    .child(cap_icon(
        chrome::icon("plus", palette::MUTED)
            .group_hover("new-terminal", |style| style.text_color(rgb(palette::TEXT))),
    ))
    .when(!compact, |row| row.child("New terminal"))
    .when(compact, |row| {
        row.w(px(32.)).mt(px(0.)).px(px(0.)).justify_center()
    })
    .when(ring, |row| {
        row.child(outline())
            .child(tip::below(label, "Inter".into()))
    })
}

pub(crate) fn who(
    name: &SharedString,
    avatar: f32,
    initial: f32,
    show_name: bool,
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
    .gap(px(7.))
    .rounded(px(8.))
    .child(cap_icon(
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
    ))
    .when(show_name, |who| {
        who.child(
            one_line(div().flex_1())
                .font_weight(FontWeight::SEMIBOLD)
                .child(name.clone()),
        )
    })
    .child(cap_icon(chrome::icon("selector", palette::FAINT)))
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
    labels: rename::Labels<'_>,
    keys: &Keys,
    window: &Window,
    cx: &mut Context<SeerWindow>,
) -> Stateful<Div> {
    let ring = keys.ring(&keys.tabs, window);
    list(div().id("strip"), keys, cx)
        .flex_none()
        .flex()
        .gap(px(2.))
        .py(px(6.))
        .px(px(STRIP_PAD))
        .overflow_x_scroll()
        .bg(rgb(palette::SURFACE))
        .border_b_1()
        .border_color(rgb(palette::LINE))
        .children(terminals.iter().map(|terminal| {
            let on = shown == Some(terminal.pane.as_str());
            let help = tip::terminal_help(&rename::display_name(terminal));
            tip::on_hover(
                selectable(terminal, Surface::Strip, cx),
                help.clone(),
                "Inter".into(),
                keys.hover() && !(on && ring) && !labels.active(terminal),
            )
            .flex_none()
            .h(px(32.))
            .pl(px(8.))
            .pr(px(12.))
            .gap(px(8.))
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap()
            .text_color(rgb(if on { palette::TEXT } else { palette::TEXT_2 }))
            .when(on, |tab| tab.bg(rgb(palette::SELECTED)))
            .when(on && ring, |tab| {
                tab.child(outline()).child(tip::below(help, "Inter".into()))
            })
            .child(cap_icon(chrome::status(terminal.agent_state)))
            .child(rename::label(terminal, Surface::Strip, labels))
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
        .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
            view.key(&event.keystroke, window, cx)
        }))
}

fn tab(
    terminal: &TerminalInfo,
    shown: Option<&str>,
    labels: rename::Labels<'_>,
    ring: bool,
    cx: &mut Context<SeerWindow>,
) -> Stateful<Div> {
    let on = shown == Some(terminal.pane.as_str());
    let name: SharedString = rename::display_name(terminal).into();
    let help = tip::terminal_help(&name);
    let row = tip::on_hover(
        selectable(terminal, Surface::Tab, cx),
        help.clone(),
        "Inter".into(),
        !(on && ring) && !labels.active(terminal),
    )
    .w_full()
    .pt(px(7.))
    .pr(px(10.))
    .pb(px(8.))
    .pl(px(10.))
    .gap(px(10.))
    .text_color(rgb(if shown == Some(terminal.pane.as_str()) {
        palette::TEXT
    } else {
        palette::TEXT_2
    }))
    .child(cap_icon(chrome::status(terminal.agent_state)))
    .child(
        div()
            .flex_1()
            .min_w_0()
            .font_weight(FontWeight::MEDIUM)
            .child(rename::label(terminal, Surface::Tab, labels)),
    )
    .children(tab_time::label(terminal).map(|time| {
        div()
            .relative()
            .top(px(-1.))
            .flex_none()
            .text_size(px(12.))
            .line_height(relative(16. / 12.))
            .font(Font {
                features: FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])),
                ..gpui::font("Inter")
            })
            .text_color(rgb(palette::FAINT))
            .child(time)
    }));
    if shown == Some(terminal.pane.as_str()) {
        row.bg(rgb(palette::SELECTED)).when(ring, |row| {
            row.child(outline()).child(tip::below(help, "Inter".into()))
        })
    } else {
        row.hover(|style| style.bg(rgb(palette::HOVER)))
    }
}

fn selectable(
    terminal: &TerminalInfo,
    surface: Surface,
    cx: &mut Context<SeerWindow>,
) -> Stateful<Div> {
    let pane = terminal.pane.clone();
    let menu_pane = pane.clone();
    div()
        .id(SharedString::from(terminal.pane.clone()))
        .relative()
        .flex()
        .items_center()
        .rounded(px(8.))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                view.press(&pane, window, cx);
                if event.click_count == 2 {
                    cx.stop_propagation();
                    view.rename(&pane, surface, window, cx);
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                view.keys_mode_pointer();
                view.name_menu(&menu_pane, surface, event.position, window, cx);
            }),
        )
}

// The design draws the focus ring as a 1 px outline with a -1 px offset, so
// the ring sits on the inner edge of the tab.
pub(crate) fn outline() -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(palette::ACCENT))
}

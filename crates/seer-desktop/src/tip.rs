use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, Context, Div, IntoElement, ParentElement, Render, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, anchored, deferred, div, point, px, relative, rgb,
};

use crate::palette;

const GAP: f32 = 4.;
const MAX_WIDTH: f32 = 280.;

struct Tip {
    text: SharedString,
    font: SharedString,
}

impl Render for Tip {
    // gpui puts the tooltip corner on the pointer. The padding moves the tip
    // below the pointer, so it does not hide the text under the pointer.
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(px(8.))
            .pt(px(20.))
            .child(tip(self.text.clone(), self.font.clone()))
    }
}

// gpui removes a shown tooltip in the first frame that draws its element
// without one.
pub(crate) fn on_hover(
    element: Stateful<Div>,
    text: SharedString,
    font: SharedString,
    show: bool,
) -> Stateful<Div> {
    element
        .when(show, |element| {
            element.tooltip(move |_, cx| {
                let tip = Tip {
                    text: text.clone(),
                    font: font.clone(),
                };
                cx.new(|_| tip).into()
            })
        })
        // gpui hides a tooltip only when it draws a frame, and a pointer
        // that leaves this element does not cause a frame.
        .on_hover(|_, window, _| window.refresh())
}

// gpui tooltips open on hover only, so an element with keyboard focus draws
// the same tip below itself.
pub(crate) fn below(text: SharedString, font: SharedString) -> Div {
    div().absolute().top_full().left_0().child(deferred(
        anchored()
            .offset(point(px(0.), px(GAP)))
            .snap_to_window_with_margin(px(GAP))
            .child(tip(text, font)),
    ))
}

// The design has no tooltip. This one has the size of the design's small
// buttons and the accent colors. gpui draws a tooltip outside the window
// root, so it does not get the root font.
fn tip(text: SharedString, font: SharedString) -> Div {
    div()
        .max_w(px(MAX_WIDTH))
        .py(px(4.6))
        .px(px(10.))
        .rounded(px(8.))
        .bg(rgb(palette::ACCENT))
        .text_color(rgb(palette::ON_ACCENT))
        .font_family(font)
        .text_size(px(12.))
        .line_height(relative(1.4))
        .child(text)
}

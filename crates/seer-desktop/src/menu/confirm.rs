use super::*;

pub(crate) struct Copy<'a> {
    pub(crate) id: &'static str,
    pub(crate) title: &'a str,
    pub(crate) body: &'a str,
    pub(crate) error: Option<&'a str>,
}

pub(crate) fn card(
    copy: Copy<'_>,
    cancel: Stateful<Div>,
    confirm: Stateful<Div>,
    window: &Window,
    cx: &gpui::App,
) -> Stateful<Div> {
    sheet(
        copy.id,
        400_f32.min(f32::from(window.viewport_size().width) - 32.),
        cx,
    )
    .p(px(20.))
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .child(
        div()
            .debug_selector({
                let title = copy.title.to_owned();
                move || title.clone()
            })
            .text_size(px(15.))
            .line_height(px(20.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(crate::label::text(copy.title.to_owned())),
    )
    .child(
        div()
            .mt(px(8.))
            .text_color(rgb(palette::MUTED))
            .child(crate::label::tooltip(copy.body.to_owned().into())),
    )
    .children(copy.error.map(|error| {
        div()
            .id("room-error")
            .debug_selector(|| "room-error".into())
            .mt(px(8.))
            .text_size(px(12.))
            .text_color(rgb(palette::DANGER))
            .child(crate::label::tooltip(error.to_owned().into()))
    }))
    .child(
        div()
            .mt(px(20.))
            .flex()
            .justify_end()
            .gap(px(8.))
            .child(cancel)
            .child(confirm),
    )
}

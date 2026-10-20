use gpui::AnyElement;

use super::*;

pub(super) const ROOM_LOST: &str = "Cannot reach the room. Shells on this computer still run.";

pub(super) fn banner(text: impl Into<SharedString>, action: Option<AnyElement>) -> Stateful<Div> {
    div()
        .id("status-banner")
        .debug_selector(|| "status-banner".into())
        .flex_none()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(12.))
        .py(px(8.))
        .bg(rgb(palette::RAISED))
        .text_color(rgb(palette::TEXT_2))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(crate::label::tooltip(text.into())),
        )
        .children(action)
}

impl SeerWindow {
    pub(super) fn connection_banner(&self) -> Option<Stateful<Div>> {
        (self.room.lost && !self.room.pending).then(|| banner(ROOM_LOST, None))
    }
}

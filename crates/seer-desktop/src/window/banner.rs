use gpui::{AnyElement, FocusHandle};

use super::*;

pub(super) const ROOM_LOST: &str = "Cannot reach the room. Shells on this computer still run.";

pub(super) enum UpdateState {
    Ready,
    Updating,
    Updated,
    Failed(String),
}

pub(super) struct VersionUi {
    refusal: Option<seer::VersionRefusal>,
    pub(super) state: UpdateState,
    pub(super) focus: FocusHandle,
    update: Option<Task<()>>,
}

impl VersionUi {
    pub(super) fn new(cx: &mut Context<SeerWindow>) -> Self {
        Self {
            refusal: None,
            state: UpdateState::Ready,
            focus: cx.focus_handle().tab_stop(true),
            update: None,
        }
    }

    pub(super) fn refused(&mut self, refusal: seer::VersionRefusal) {
        self.refusal = Some(refusal);
    }

    pub(super) fn connected(&mut self) {
        if matches!(self.state, UpdateState::Ready | UpdateState::Failed(_)) {
            self.refusal = None;
            self.state = UpdateState::Ready;
        }
    }

    pub(super) fn can_update(&self) -> bool {
        self.refusal.is_some() && matches!(self.state, UpdateState::Ready | UpdateState::Failed(_))
    }

    fn text(&self) -> Option<String> {
        match &self.state {
            UpdateState::Ready | UpdateState::Updating => self.refusal.as_ref().map(|refusal| {
                format!(
                    "This room uses Seer {}. You have {}.",
                    refusal.server, refusal.client
                )
            }),
            UpdateState::Updated => Some("Seer is updated. Reopen Seer to use it.".into()),
            UpdateState::Failed(error) => Some(error.clone()),
        }
    }
}

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
    pub(super) fn status_banner(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if let Some(text) = self.room.version.text() {
            let action = (!matches!(self.room.version.state, UpdateState::Updated))
                .then(|| self.update_button(window, cx).into_any_element());
            return Some(banner(text, action).min_h(px(48.)));
        }
        (self.room.lost && !self.room.pending).then(|| banner(ROOM_LOST, None))
    }

    fn update_button(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        use gpui::StatefulInteractiveElement;
        let updating = matches!(self.room.version.state, UpdateState::Updating);
        let button = crate::menu::button(
            "version-update",
            if updating { "Updating..." } else { "Update" },
            &self.room.version.focus,
            None,
            self.keys.mode,
            window,
        )
        .flex_none()
        .when(updating, |button| button.tab_stop(false))
        .when(!updating, |button| {
            button.on_click(cx.listener(|view, _, window, cx| {
                view.begin_version_update(window, cx);
            }))
        });
        div()
            .flex_none()
            .when(updating, |action| action.opacity(0.6))
            .child(button)
    }

    pub(super) fn version_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.room.version.focus.is_focused(window)
            || key.modifiers.control
            || key.modifiers.alt
            || key.modifiers.platform
            || key.modifiers.function
        {
            return false;
        }
        self.keys.mode = Mode::Keyboard;
        match key.key.as_str() {
            "enter" | "space" => self.begin_version_update(window, cx),
            "tab" if key.modifiers.shift => window.focus(&self.keys.tabs),
            "tab" | "escape" => window.focus(&self.input.focus),
            _ => return false,
        }
        cx.notify();
        true
    }

    fn begin_version_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.room.version.can_update() {
            return;
        }
        self.room.version.state = UpdateState::Updating;
        if self.room.version.focus.is_focused(window) {
            window.focus(&self.input.focus);
        }
        let work = cx.background_executor().spawn(async { seer::update() });
        self.room.version.update = Some(cx.spawn(async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.room.version.state = match result {
                    Ok(()) => UpdateState::Updated,
                    Err(error) => UpdateState::Failed(error),
                };
                view.notify_room(cx);
            });
        }));
        self.notify_room(cx);
    }
}

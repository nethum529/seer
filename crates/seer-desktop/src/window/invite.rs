use gpui::{AppContext, Entity, FocusHandle, Subscription, actions};

use super::*;
use crate::invite::{Closed, InviteSheet};

actions!(seer, [OpenInvite]);

pub(super) struct OpenSheet {
    pub(super) view: Entity<InviteSheet>,
    _events: Subscription,
    _updates: Subscription,
    restore: Option<FocusHandle>,
}

impl SeerWindow {
    pub(crate) fn invite_open(&self) -> bool {
        self.invite.is_some()
    }

    pub(super) fn show_invite(
        &mut self,
        _: &OpenInvite,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.content, Content::Terminal(_, _)) {
            return;
        }
        let keyboard = self.keys.mode == Mode::Keyboard;
        if let Some(sheet) = &self.invite {
            sheet
                .view
                .update(cx, |sheet, cx| sheet.reopen(keyboard, window, cx));
            return;
        }
        let Ok(store) = ServerStore::load() else {
            return;
        };
        let Some(server) = store.default_server().cloned() else {
            return;
        };
        let restore = window.focused(cx);
        let view = cx.new(|cx| InviteSheet::new(server, keyboard, window, cx));
        let events = cx.subscribe_in(&view, window, |view, _, _: &Closed, window, cx| {
            if let Some(sheet) = view.invite.take()
                && let Some(focus) = sheet.restore
            {
                window.focus(&focus);
            }
            view.notify_invite(cx);
        });
        let updates = cx.observe(&view, |view, _, cx| view.notify_invite(cx));
        self.invite = Some(OpenSheet {
            view,
            _events: events,
            _updates: updates,
            restore,
        });
        self.notify_invite(cx);
    }

    fn notify_invite(&self, cx: &mut Context<Self>) {
        cx.notify();
        #[cfg(target_os = "macos")]
        if let Some(frame) = &self.input.frame {
            frame.request(cx);
        }
    }
}

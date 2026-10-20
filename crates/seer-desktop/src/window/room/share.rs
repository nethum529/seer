use std::time::Duration;

use futures::future::{Either, select};

use super::*;

impl SeerWindow {
    pub(super) fn room_actions(&self) -> Vec<MenuAction> {
        let mut actions = self.room.actions();
        if let Content::Terminal(link, _) = &self.content
            && link.people.people.len() > 1
        {
            let index = usize::from(actions.first() == Some(&MenuAction::Invite));
            actions.splice(
                index..index,
                [MenuAction::ShareAll, MenuAction::StopSharing],
            );
        }
        actions
    }

    pub(super) fn confirm_share_all(
        &mut self,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cancel = cx.focus_handle();
        let confirm = cx.focus_handle();
        window.focus(&cancel);
        self.room.sheet = Some(Sheet {
            action: Confirmation::ShareAll,
            return_focus: self.room.focus.clone(),
            cancel,
            confirm,
            motion: Motion::open(keyboard, 200),
        });
        self.notify_room(cx);
    }

    pub(super) fn run_all_grants(
        &mut self,
        can_type: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.room.pending {
            return;
        }
        let Content::Terminal(link, _) = &mut self.content else {
            self.room.error = Some("Room is offline. Try again.".into());
            self.notify_room(cx);
            return;
        };
        let response = link.people.set_all_grants(can_type);
        self.room.pending = true;
        self.room.error = None;
        if let Some(sheet) = &self.room.sheet {
            window.focus(&sheet.confirm);
        } else {
            window.focus(&self.room.focus);
        }
        let timeout = cx.background_executor().timer(Duration::from_secs(5));
        self.room.action = Some(cx.spawn_in(window, async move |view, cx| {
            let result = match select(response, timeout).await {
                Either::Left((result, _)) => result.unwrap_or(Err("Room is offline. Try again.")),
                Either::Right(_) => Err("The room did not answer. Try again."),
            };
            let _ = view.update_in(cx, |view, window, cx| {
                if let Content::Terminal(link, _) = &mut view.content {
                    link.people.finish_all_grants(result);
                }
                view.room.pending = false;
                if let Err(error) = result {
                    if !view.room.modal() {
                        view.open_room(view.keys.mode == Mode::Keyboard, window, cx);
                    }
                    view.room.error = Some(error.into());
                } else if view.room.modal() {
                    view.close_room_sheet(view.keys.mode == Mode::Keyboard, window, cx);
                }
                view.notify_room(cx);
            });
        }));
        self.notify_room(cx);
    }
}

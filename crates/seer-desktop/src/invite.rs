use std::time::{Duration, Instant};

use gpui::{
    AppContext, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, KeyDownEvent,
    Subscription, Task, Window,
};
use seer::{IssuedInvite, ServerEntry};

use crate::motion::Motion;
use crate::name_input::{Finished, NameInput};

mod paint;

pub(crate) struct Closed;

pub(crate) struct InviteSheet {
    server: ServerEntry,
    hours: u32,
    custom: bool,
    input: Entity<NameInput>,
    close_focus: FocusHandle,
    duration_focus: FocusHandle,
    primary_focus: FocusHandle,
    copy_focus: [FocusHandle; 2],
    keyboard: bool,
    pending: bool,
    error: Option<&'static str>,
    invitation: Option<IssuedInvite>,
    copied: [Option<Instant>; 2],
    motion: Motion,
    copy_motion: [Motion; 2],
    closed: bool,
    _input_events: Subscription,
    _input_updates: Subscription,
    work: Option<Task<()>>,
}

impl EventEmitter<Closed> for InviteSheet {}

impl InviteSheet {
    pub(crate) fn new(
        server: ServerEntry,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| NameInput::form("48".into(), false, cx));
        let events = cx.subscribe_in(&input, window, |sheet, _, event, window, cx| {
            if matches!(event, Finished::Save) {
                sheet.create(window, cx);
            }
        });
        let updates = cx.observe(&input, |_, _, cx| cx.notify());
        let duration_focus = cx.focus_handle();
        window.focus(&duration_focus);
        Self {
            server,
            hours: 24,
            custom: false,
            input,
            close_focus: cx.focus_handle(),
            duration_focus,
            primary_focus: cx.focus_handle(),
            copy_focus: [cx.focus_handle(), cx.focus_handle()],
            keyboard,
            pending: false,
            error: None,
            invitation: None,
            copied: [None, None],
            copy_motion: [Motion::feedback(), Motion::feedback()],
            motion: Motion::open(keyboard, 200),
            closed: false,
            _input_events: events,
            _input_updates: updates,
            work: None,
        }
    }

    pub(crate) fn reopen(&mut self, keyboard: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.motion.reopen(keyboard, 200);
        self.keyboard = keyboard;
        self.hours = 24;
        self.custom = false;
        self.pending = false;
        self.error = None;
        self.invitation = None;
        self.copied = [None, None];
        self.copy_motion = [Motion::feedback(), Motion::feedback()];
        self.work = None;
        self.closed = false;
        window.focus(&self.duration_focus);
        cx.notify();
    }

    fn close(&mut self, keyboard: bool, cx: &mut Context<Self>) {
        self.motion.close(keyboard, 140);
        self.work = None;
        cx.notify();
    }

    fn pick(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending {
            return;
        }
        self.custom = index == 3;
        self.error = None;
        self.input.update(cx, |input, cx| input.invalid(false, cx));
        if let Some(hours) = [1, 24, 168].get(index) {
            self.hours = *hours;
        } else {
            let hours = if self.hours == 24 { 48 } else { self.hours };
            self.input
                .update(cx, |input, cx| input.set_text(hours.to_string(), cx));
        }
        window.focus(&self.duration_focus);
        cx.notify();
    }

    fn selected(&self) -> usize {
        if self.custom {
            3
        } else {
            [1, 24, 168]
                .iter()
                .position(|hours| *hours == self.hours)
                .unwrap_or(1)
        }
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.invitation.is_some() || self.motion.closing() {
            return;
        }
        let hours = if self.custom {
            self.input
                .read(cx)
                .text
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|hours| (1..=168).contains(hours))
        } else {
            Some(self.hours)
        };
        let Some(hours) = hours else {
            self.error = Some("Choose 1 to 168 hours.");
            self.input.update(cx, |input, cx| input.invalid(true, cx));
            window.focus(&self.input.read(cx).focus);
            cx.notify();
            return;
        };
        self.hours = hours;
        self.pending = true;
        self.error = None;
        self.input.update(cx, |input, cx| input.invalid(false, cx));
        window.focus(&self.primary_focus);
        let server = self.server.clone();
        let request = cx
            .background_executor()
            .spawn(async move { seer::create_invite(&server, Some(hours)) });
        self.work = Some(cx.spawn_in(window, async move |view, cx| {
            let result = request.await;
            let _ = view.update_in(cx, |sheet, window, cx| {
                sheet.pending = false;
                match result {
                    Ok(invitation) => {
                        sheet.invitation = Some(invitation);
                        window.focus(&sheet.copy_focus[0]);
                    }
                    Err(_) => sheet.error = Some("Could not create the invite."),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn copy(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(invitation) = &self.invitation else {
            return;
        };
        let line = if index == 0 {
            invitation.join_line()
        } else {
            invitation.install_line()
        };
        cx.write_to_clipboard(ClipboardItem::new_string(line));
        let copied = Instant::now();
        self.copied[index] = Some(copied);
        self.copy_motion[index].reopen(self.keyboard, 300);
        let delay = cx.background_executor().timer(Duration::from_millis(1600));
        cx.spawn(async move |view, cx| {
            delay.await;
            let _ = view.update(cx, |sheet, cx| {
                if sheet.copied[index] == Some(copied) {
                    sheet.copied[index] = None;
                    sheet.copy_motion[index].close(sheet.keyboard, 300);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn focus_order(&self, cx: &Context<Self>) -> Vec<FocusHandle> {
        let mut order = vec![self.close_focus.clone()];
        if self.invitation.is_some() {
            order.extend(self.copy_focus.iter().cloned());
        } else if !self.pending {
            order.push(self.duration_focus.clone());
            if self.custom {
                order.push(self.input.read(cx).focus.clone());
            }
        }
        order.push(self.primary_focus.clone());
        order
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.keyboard = true;
        let key = &event.keystroke;
        match key.key.as_str() {
            "escape" => self.close(true, cx),
            "tab" => {
                let order = self.focus_order(cx);
                let index = order
                    .iter()
                    .position(|focus| focus.is_focused(window))
                    .unwrap_or(0);
                let next = if key.modifiers.shift {
                    (index + order.len() - 1) % order.len()
                } else {
                    (index + 1) % order.len()
                };
                window.focus(&order[next]);
            }
            "left" | "right" if self.duration_focus.is_focused(window) => {
                let next = (self.selected() + if key.key == "left" { 3 } else { 1 }) % 4;
                self.pick(next, window, cx);
            }
            "enter" | "space" => {
                if self.input.read(cx).focus.is_focused(window) {
                    return;
                }
                if self.close_focus.is_focused(window) {
                    self.close(true, cx);
                } else if self.primary_focus.is_focused(window) {
                    if self.invitation.is_some() {
                        self.close(true, cx);
                    } else {
                        self.create(window, cx);
                    }
                } else if let Some(index) = self
                    .copy_focus
                    .iter()
                    .position(|focus| focus.is_focused(window))
                {
                    self.copy(index, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.motion.gone() && !self.closed {
            self.closed = true;
            cx.emit(Closed);
        }
        let copying = self.copy_motion.iter().any(Motion::animating);
        if self.motion.animating() || copying {
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }
    }
}

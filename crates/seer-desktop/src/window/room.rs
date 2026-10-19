use std::cell::Cell;
use std::rc::Rc;

use gpui::{Bounds, FocusHandle, Pixels};
use seer::ServerEntry;
use seer::rooms::{RoomAction, RoomState, RoomSummary};

use super::*;
use crate::motion::Motion;

mod paint;
mod sheet;

pub(super) const STOPPED: &str = "The room is stopped. Open the room menu to restore it.";

pub(super) struct RoomUi {
    pub(super) server: Option<ServerEntry>,
    pub(super) current: Option<RoomSummary>,
    pub(super) can_host: bool,
    others: Vec<RoomSummary>,
    pub(super) focus: FocusHandle,
    anchor: Rc<Cell<Bounds<Pixels>>>,
    menu: Option<Menu>,
    sheet: Option<Sheet>,
    pending: bool,
    error: Option<String>,
    refresh: Option<Task<()>>,
    action: Option<Task<()>>,
}

struct Menu {
    focus: FocusHandle,
    selected: Option<usize>,
    motion: Motion,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    Invite,
    Room(RoomAction),
}

#[derive(Clone)]
enum Confirmation {
    Room(RoomAction, Option<Box<RoomSummary>>),
    Terminal { pane: String, program: String },
}

struct Sheet {
    action: Confirmation,
    return_focus: FocusHandle,
    cancel: FocusHandle,
    confirm: FocusHandle,
    motion: Motion,
}

impl RoomUi {
    pub(super) fn new(server: Option<ServerEntry>, cx: &mut Context<SeerWindow>) -> Self {
        Self {
            server,
            current: None,
            can_host: cfg!(target_os = "linux"),
            others: Vec::new(),
            focus: cx.focus_handle().tab_stop(true),
            anchor: Rc::default(),
            menu: None,
            sheet: None,
            pending: false,
            error: None,
            refresh: None,
            action: None,
        }
    }

    fn actions(&self) -> Vec<MenuAction> {
        let Some(room) = &self.current else {
            return if self.server.is_none() && self.can_host {
                vec![MenuAction::Room(RoomAction::Start)]
            } else {
                Vec::new()
            };
        };
        if !room.host {
            vec![MenuAction::Room(RoomAction::Leave)]
        } else if room.state == RoomState::Unreachable {
            vec![
                MenuAction::Room(RoomAction::Restore),
                MenuAction::Room(RoomAction::StartNew),
            ]
        } else {
            vec![MenuAction::Invite, MenuAction::Room(RoomAction::Stop)]
        }
    }

    pub(super) fn modal(&self) -> bool {
        self.sheet
            .as_ref()
            .is_some_and(|sheet| !sheet.motion.closing())
    }

    pub(super) fn open(&self) -> bool {
        self.menu
            .as_ref()
            .is_some_and(|menu| !menu.motion.closing())
    }
}

impl SeerWindow {
    pub(crate) fn room_blocks_terminal_pointer(&self) -> bool {
        self.appearance.motion.is_some()
            || self.room.menu.is_some()
            || self.room.sheet.is_some()
            || self.people_menu.is_some()
            || self.invite_open()
    }

    pub(super) fn notify_room(&self, cx: &mut Context<Self>) {
        cx.notify();
        #[cfg(target_os = "macos")]
        if let Some(frame) = &self.input.frame {
            frame.request(cx);
        }
    }

    pub(super) fn refresh_room(&mut self, cx: &mut Context<Self>) {
        let Some(server) = self.room.server.clone() else {
            return;
        };
        let work = cx.background_executor().spawn(async move {
            let mut others = seer::rooms::saved_rooms()?;
            let current = others
                .iter()
                .position(|room| room.server.user_id == server.user_id)
                .map(|index| others.remove(index))
                .unwrap_or_else(|| seer::rooms::inspect_room(server));
            Ok::<_, io::Error>((current, others))
        });
        self.room.refresh = Some(cx.spawn(async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                match result {
                    Ok((mut current, others)) => {
                        if current.state == RoomState::Unreachable
                            && let Some(previous) = &view.room.current
                        {
                            current.host |= previous.host;
                            current.people = previous.people.clone();
                        }
                        view.room.current = Some(current);
                        view.room.others = others;
                    }
                    Err(error) => view.room.error = Some(error.to_string()),
                }
                view.notify_room(cx);
            });
        }));
    }

    fn open_room(&mut self, keyboard: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.room.pending || self.room.modal() || self.invite_open() {
            return;
        }
        self.input.reset();
        self.menu = None;
        self.people_menu = None;
        self.room.error = None;
        self.keys.mode = if keyboard {
            Mode::Keyboard
        } else {
            Mode::Pointer
        };
        if let Some(menu) = &mut self.room.menu {
            menu.motion.reopen(keyboard, 150);
            menu.selected = keyboard.then_some(0);
            window.focus(&menu.focus);
        } else {
            let focus = cx.focus_handle();
            window.focus(&focus);
            self.room.menu = Some(Menu {
                focus,
                selected: keyboard.then_some(0),
                motion: Motion::open(keyboard, 150),
            });
        }
        self.refresh_room(cx);
        self.notify_room(cx);
    }

    pub(super) fn close_room_menu(
        &mut self,
        keyboard: bool,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(menu) = &mut self.room.menu
            && !menu.motion.closing()
        {
            menu.motion.close(keyboard, 80);
        }
        if focus {
            window.focus(&self.room.focus);
        }
        self.notify_room(cx);
    }

    fn pick_room_action(
        &mut self,
        action: MenuAction,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.room.pending || !self.room.open() || !self.room.actions().contains(&action) {
            return;
        }
        self.room.error = None;
        self.keys.mode = if keyboard {
            Mode::Keyboard
        } else {
            Mode::Pointer
        };
        self.close_room_menu(keyboard, false, window, cx);
        let MenuAction::Room(action) = action else {
            window.focus(&self.room.focus);
            self.show_invite(&super::invite::OpenInvite, window, cx);
            return;
        };
        if action == RoomAction::Restore {
            self.run_room_action(action, window, cx);
            return;
        }
        let cancel = cx.focus_handle();
        let confirm = cx.focus_handle();
        window.focus(&cancel);
        self.room.sheet = Some(Sheet {
            action: Confirmation::Room(action, self.room.current.clone().map(Box::new)),
            return_focus: self.room.focus.clone(),
            cancel,
            confirm,
            motion: Motion::open(keyboard, 200),
        });
        self.notify_room(cx);
    }

    fn close_room_sheet(&mut self, keyboard: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.room.pending || !self.room.modal() {
            return;
        }
        if let Some(sheet) = &mut self.room.sheet {
            sheet.motion.close(keyboard, 140);
        }
        self.keys.mode = if keyboard {
            Mode::Keyboard
        } else {
            Mode::Pointer
        };
        if let Some(sheet) = &self.room.sheet {
            window.focus(&sheet.return_focus);
        }
        self.notify_room(cx);
    }

    pub(crate) fn room_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.room.modal() {
            self.sheet_key(key, window, cx);
            return true;
        }
        if self.room.sheet.is_some() {
            return true;
        }
        if self.room.open() {
            self.menu_key(key, window, cx);
            return true;
        }
        if self.room.focus.is_focused(window)
            && matches!(key.key.as_str(), "enter" | "space" | "up" | "down")
        {
            self.open_room(true, window, cx);
            return true;
        }
        false
    }

    fn menu_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        self.keys.mode = Mode::Keyboard;
        let actions = self.room.actions();
        let Some(menu) = &mut self.room.menu else {
            return;
        };
        let selected = menu
            .selected
            .unwrap_or(0)
            .min(actions.len().saturating_sub(1));
        match key.key.as_str() {
            "escape" | "tab" => self.close_room_menu(true, true, window, cx),
            "enter" | "space" => {
                if let Some(action) = actions.get(selected) {
                    self.pick_room_action(*action, true, window, cx);
                }
            }
            "up" | "down" | "home" | "end" => {
                menu.selected = sidebar::step(&key.key, actions.len(), menu.selected);
            }
            _ => {}
        }
        self.notify_room(cx);
    }

    fn sheet_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        self.keys.mode = Mode::Keyboard;
        let Some(sheet) = &self.room.sheet else {
            return;
        };
        match key.key.as_str() {
            "escape" => self.close_room_sheet(true, window, cx),
            "tab" => {
                window.focus(if sheet.cancel.is_focused(window) {
                    &sheet.confirm
                } else {
                    &sheet.cancel
                });
            }
            "enter" | "space" if !self.room.pending => {
                if sheet.confirm.is_focused(window) {
                    self.run_confirmation(window, cx);
                } else {
                    self.close_room_sheet(true, window, cx);
                }
            }
            _ => {}
        }
        self.notify_room(cx);
    }

    fn run_room_action(&mut self, action: RoomAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.room.pending {
            return;
        }
        let server = self.room.server.clone();
        self.room.pending = true;
        self.room.error = None;
        self.room.refresh = None;
        if let Some(sheet) = &self.room.sheet {
            window.focus(&sheet.confirm);
        } else {
            window.focus(&self.room.focus);
        }
        let work = cx.background_executor().spawn(async move {
            action.run(server.as_ref())?;
            if matches!(
                action,
                RoomAction::Start | RoomAction::Restore | RoomAction::StartNew
            ) {
                ServerStore::load()?
                    .default_server()
                    .map(|server| {
                        let opened = join::open_server(server)?;
                        Ok::<_, io::Error>((opened, seer::rooms::inspect_room(server.clone())))
                    })
                    .transpose()
            } else {
                Ok(None)
            }
        });
        self.room.action = Some(cx.spawn_in(window, async move |view, cx| {
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.room.pending = false;
                match result {
                    Ok(opened) => {
                        view.room_action_done(action, opened, cx);
                        window.focus(&view.room.focus);
                    }
                    Err(error) => {
                        if action == RoomAction::Restore {
                            view.open_room(view.keys.mode == Mode::Keyboard, window, cx);
                        }
                        view.room.error = Some(if action == RoomAction::Start {
                            "Could not start the room. Try again.".into()
                        } else {
                            error.to_string()
                        });
                    }
                }
                view.notify_room(cx);
            });
        }));
        self.notify_room(cx);
    }

    fn room_action_done(
        &mut self,
        action: RoomAction,
        opened: Option<(Opened, RoomSummary)>,
        cx: &mut Context<Self>,
    ) {
        if let Some(sheet) = &mut self.room.sheet {
            sheet.motion.close(self.keys.mode == Mode::Keyboard, 140);
        }
        self.room.menu = None;
        self.input.reset();
        match action {
            RoomAction::Stop => {
                self.content = Content::Message(STOPPED.into());
                self._messages = None;
                if let Some(room) = &mut self.room.current {
                    room.state = RoomState::Unreachable;
                }
            }
            RoomAction::Leave => {
                self.content = Content::Message(NO_ROOM.into());
                self._messages = None;
                self.server = None;
                self.room.server = None;
                self.room.current = None;
            }
            RoomAction::Start | RoomAction::Restore | RoomAction::StartNew => {
                if let Some(((link, server, messages), summary)) = opened {
                    self.content = Content::Terminal(Box::new(link), server.name.clone().into());
                    self.server = Some(server.clone());
                    self.room.server = Some(server);
                    self.room.current = Some(summary);
                    self._join = None;
                    self.listen(messages, cx);
                }
                self.refresh_room(cx);
            }
        }
    }

    fn room_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .room
            .menu
            .as_ref()
            .is_some_and(|menu| menu.motion.gone())
        {
            self.room.menu = None;
        }
        if self
            .room
            .sheet
            .as_ref()
            .is_some_and(|sheet| sheet.motion.gone())
        {
            self.room.sheet = None;
        }
        let moving = self
            .room
            .menu
            .as_ref()
            .is_some_and(|menu| menu.motion.animating())
            || self
                .room
                .sheet
                .as_ref()
                .is_some_and(|sheet| sheet.motion.animating());
        if moving {
            cx.on_next_frame(window, |view, _, cx| view.notify_room(cx));
        }
    }
}

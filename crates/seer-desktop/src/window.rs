use std::io;

use futures::StreamExt;
use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Div, Entity, Font, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    Keystroke, MouseButton, ParentElement, Render, ScrollHandle, SharedString, Stateful, Styled,
    Subscription, Task, Window, div, px, relative, rgb,
};
use seer::{ServerEntry, ServerStore};

use crate::chrome;
use crate::input::Input;
use crate::link::{Event, Link, Messages, Terminal};
use crate::palette;
use crate::screen::{self, Rows, Screen};
use crate::shortcuts::{self, Action, Scope};
use crate::sidebar::{self, Keys, Mode};
use crate::tab_time;
use crate::tip;

mod banner;
mod commands;
mod invite;
mod join;
mod paint;
mod people;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod people_e2e;
mod read_only_title;
pub(crate) mod rename;
mod reveal;
mod room;
pub(crate) mod settings;
pub(crate) mod sidebar_visibility;
mod slide;
mod start;
mod switch;
pub(crate) mod terminals;
pub(crate) mod top_bar;
use rename::Surface;

const LINK_LOST: &str = "The link to your terminals stopped. Open Seer again.";
// The design's narrow layout starts below this window width.
const NARROW: f32 = 720.;

pub(crate) enum Content {
    Start(start::StartScreen),
    Message(SharedString),
    Terminal(Box<Link>, SharedString),
}

pub(crate) struct SeerWindow {
    pub(crate) content: Content,
    join_screen: Option<Entity<crate::join::Join>>,
    font: Font,
    font_size: f32,
    ui_font: SharedString,
    pub(crate) keys: Keys,
    pub(crate) input: Input,
    rows: Rows,
    appearance: crate::appearance::Appearance,
    room: room::RoomUi,
    editing: Option<rename::Editing>,
    menu: Option<rename::Menu>,
    people_menu: Option<people::Menu>,
    invite: Option<invite::OpenSheet>,
    server: Option<ServerEntry>,
    _messages: Option<Task<()>>,
    _join: Option<Subscription>,
    _tab_time: Option<Task<()>>,
}

impl SeerWindow {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (font, font_size) = crate::terminal_font::load(cx);
        let start = start::StartScreen::new(cx);
        window.focus(&start.host);
        let keys = Keys {
            tabs: cx.focus_handle().tab_stop(true),
            who: cx.focus_handle().tab_stop(true),
            new_terminal: cx.focus_handle().tab_stop(true),
            scroll: ScrollHandle::new(),
            mode: Mode::Pointer,
        };
        let input = Input::new(window, cx);
        let mut view = Self {
            content: Content::Start(start),
            join_screen: None,
            font,
            font_size,
            ui_font: "Inter".into(),
            keys,
            input,
            rows: Rows::default(),
            appearance: crate::appearance::Appearance::load(seer::Settings::path().ok(), cx),
            room: room::RoomUi::new(None, cx),
            editing: None,
            menu: None,
            people_menu: None,
            invite: None,
            server: None,
            _messages: None,
            _join: None,
            _tab_time: Some(tab_time::refresh(cx)),
        };
        view.refresh_room(cx);
        view
    }

    fn listen(&mut self, mut messages: Messages, cx: &mut Context<Self>) {
        self._messages = Some(cx.spawn(async move |this, cx| {
            while let Some(message) = messages.next().await {
                if this
                    .update(cx, |view, cx| view.receive(message, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(crate) fn keys_mode_pointer(&mut self) {
        self.keys.mode = Mode::Pointer;
    }

    pub(crate) fn screen(&mut self) -> Option<(&Terminal, &mut Rows)> {
        match &self.content {
            Content::Terminal(link, _) => Some((link.terminal(), &mut self.rows)),
            Content::Message(_) | Content::Start(_) => None,
        }
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16, cx: &mut Context<Self>) {
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.resize(cols, rows),
            Content::Message(_) | Content::Start(_) => Ok(()),
        };
        self.check(result, cx);
    }

    pub(crate) fn select(&mut self, pane: &str, cx: &mut Context<Self>) {
        self.input.reset();
        self.rows = Rows::default();
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.select(pane),
            Content::Message(_) | Content::Start(_) => Ok(()),
        };
        self.check(result, cx);
        self.notify_room(cx);
    }

    pub(crate) fn press(&mut self, pane: &str, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.keys.tabs);
        self.select(pane, cx);
    }

    pub(crate) fn key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.rename_key(keystroke, window, cx) {
            cx.stop_propagation();
            return;
        }
        if !self.keys.tabs.is_focused(window) {
            return;
        }
        let Content::Terminal(link, _) = &self.content else {
            return;
        };
        let held = keystroke.modifiers;
        if held.control || held.alt || held.platform || held.function {
            return;
        }
        let terminals = link.terminals();
        let now = terminals
            .iter()
            .position(|terminal| Some(terminal.pane.as_str()) == link.shown());
        let Some((at, terminal)) = sidebar::step(&keystroke.key, terminals.len(), now)
            .and_then(|at| Some((at, terminals.get(at)?)))
        else {
            return;
        };
        let pane = terminal.pane.clone();
        self.keys.reveal(at);
        self.keys.mode = Mode::Keyboard;
        self.select(&pane, cx);
    }

    fn travel(&mut self, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_key(keystroke, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.invite.is_some() {
            return;
        }
        if self.window_command_key(keystroke, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.room_key(keystroke, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.join_travel(keystroke, window, cx) {
            return;
        }
        if self.start_key(keystroke, window, cx) {
            return;
        }
        if self.sidebar_key(keystroke, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.people_key(keystroke, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.new_terminal_key(keystroke, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.switch_terminal_key(keystroke, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.version_key(keystroke, window, cx) {
            cx.stop_propagation();
            return;
        }
        self.focus_key(keystroke, window, cx);
    }

    fn focus_key(&mut self, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.focus.is_focused(window) || self.editing.is_some() || self.menu.is_some() {
            return;
        }
        self.keys.mode = match keystroke.key.as_str() {
            "tab" if keystroke.modifiers.shift => {
                window.focus_prev();
                Mode::Keyboard
            }
            "tab" if self.keys.tabs.is_focused(window) => {
                window.focus(if self.room.version.can_update() {
                    &self.room.version.focus
                } else {
                    &self.input.focus
                });
                Mode::Keyboard
            }
            "tab" => {
                window.focus_next();
                Mode::Keyboard
            }
            "escape" => Mode::Quiet,
            _ => return,
        };
        cx.stop_propagation();
        cx.notify();
    }

    fn receive(&mut self, event: io::Result<Event>, cx: &mut Context<Self>) {
        let room = matches!(event, Ok(Event::Room(_)));
        match &event {
            Ok(Event::Room(crate::room::transport::RoomEvent::VersionMismatch(refusal))) => {
                self.room.version.refused(refusal.clone());
            }
            Ok(Event::Room(crate::room::transport::RoomEvent::Lost)) => self.room.lost = true,
            Ok(Event::Room(crate::room::transport::RoomEvent::Connected(_))) => {
                self.room.lost = false;
                self.room.version.connected();
            }
            Ok(Event::Room(crate::room::transport::RoomEvent::Removed)) => {
                self.removed_from_room(cx);
            }
            Ok(Event::Room(crate::room::transport::RoomEvent::Stopped)) => {
                self.room.lost = false;
                self.room_action_done(seer::rooms::RoomAction::Stop, None, cx);
            }
            _ => {}
        }
        #[cfg(target_os = "macos")]
        let echo = matches!(event, Ok(Event::Screen)) && self.input.waiting_echo;
        #[cfg(target_os = "macos")]
        let metadata = matches!(
            event,
            Ok(Event::Message(
                seer_core::proto::ServerMsg::Terminals { .. }
            ))
        );
        let result = match &mut self.content {
            Content::Terminal(link, _) => {
                let selected = (
                    link.people.selected.clone(),
                    link.can_input(),
                    link.shown().map(str::to_owned),
                );
                let result = event.and_then(|event| link.receive(event));
                if let Some(menu) = &mut self.people_menu
                    && !link
                        .people
                        .people
                        .iter()
                        .any(|person| person.user_id == menu.highlighted)
                {
                    menu.highlighted.clone_from(&link.people.selected);
                }
                if selected
                    != (
                        link.people.selected.clone(),
                        link.can_input(),
                        link.shown().map(str::to_owned),
                    )
                {
                    self.input.reset();
                    self.rows = Rows::default();
                }
                result
            }
            Content::Message(_) | Content::Start(_) => Ok(()),
        };
        self.check(result, cx);
        if room {
            self.notify_room(cx);
        } else {
            cx.notify();
        }
        #[cfg(target_os = "macos")]
        if echo {
            self.input.waiting_echo = false;
        }
        #[cfg(target_os = "macos")]
        if (echo || metadata)
            && let Some(frame) = &self.input.frame
        {
            frame.request(cx);
        }
    }

    pub(crate) fn check(&mut self, result: io::Result<()>, cx: &mut Context<Self>) {
        if result.is_err() {
            self.content = Content::Message(LINK_LOST.into());
            cx.notify();
        }
    }
}

type Opened = (Link, ServerEntry, Messages);

#[cfg(all(test, not(feature = "native-text-e2e")))]
mod edit_keys_e2e;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod input_e2e;

#[cfg(all(test, not(feature = "native-text-e2e")))]
mod commands_e2e;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod control_e2e;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod invite_e2e;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod room_invite_e2e;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod share_all_e2e;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod watch_e2e;

#[cfg(all(test, not(feature = "native-text-e2e")))]
mod settings_e2e;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod switch_e2e;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod terminals_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
pub(crate) mod native_text_e2e;
#[cfg(all(test, not(feature = "native-text-e2e")))]
mod top_bar_e2e;

#[cfg(all(test, not(feature = "native-text-e2e")))]
mod sidebar_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod sidebar_native_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod shortcuts_native_e2e;

#[cfg(all(test, not(feature = "native-text-e2e")))]
mod rebind_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod rebind_native_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod spaces_native_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod recovery_native_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod switch_native_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod commands_native_e2e;

#[cfg(all(test, not(feature = "native-text-e2e")))]
mod start_room_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod start_room_native_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod appearance_native_e2e;
#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod appearance_pixel_samples;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod legacy_appearance_native_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod banner_native_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod version_mirror;
#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod version_native_e2e;

#[cfg(all(test, not(feature = "native-text-e2e")))]
mod start_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod start_native_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod start_review_native_e2e;

#[cfg(all(test, not(feature = "native-text-e2e")))]
mod remove_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod remove_native_e2e;
#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod share_all_fixture;
#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod share_all_native_e2e;

#[cfg(all(test, not(feature = "native-text-e2e")))]
mod processes_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod processes_native_e2e;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod processes_pixel_samples;

#[cfg(all(test, feature = "native-text-e2e", target_os = "macos"))]
mod font_native_e2e;

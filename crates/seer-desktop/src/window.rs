use std::io;

use futures::StreamExt;
use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, Context, Div, Entity, Font, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, Keystroke, MouseButton, ParentElement, Render, ScrollHandle, SharedString,
    Stateful, Styled, Subscription, Task, Window, div, px, relative, rgb,
};
use seer::ServerStore;

use crate::chrome;
use crate::input::Input;
use crate::link::{Event, Link, Messages, Terminal};
use crate::palette;
use crate::screen::{self, Rows, Screen};
use crate::sidebar::{self, Keys, Mode};
use crate::tab_time;
use crate::tip;

mod join;
pub(crate) mod rename;
use rename::Surface;

const LINK_LOST: &str = "The link to your terminals stopped. Open Seer again.";
// The design's narrow layout starts below this window width.
const NARROW: f32 = 720.;

pub(crate) enum Content {
    Join(Entity<crate::join::Join>),
    Message(SharedString),
    Terminal(Box<Link>, SharedString),
}

pub(crate) struct SeerWindow {
    pub(crate) content: Content,
    font: Font,
    ui_font: SharedString,
    pub(crate) keys: Keys,
    pub(crate) input: Input,
    rows: Rows,
    editing: Option<rename::Editing>,
    menu: Option<rename::Menu>,
    _messages: Option<Task<()>>,
    _join: Option<Subscription>,
    _tab_time: Option<Task<()>>,
}

impl SeerWindow {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let font = screen::terminal_font();
        let (content, messages) = match open() {
            Ok(Some((link, name, messages))) => {
                (Content::Terminal(Box::new(link), name), Some(messages))
            }
            Ok(None) => (
                Content::Join(cx.new(|cx| crate::join::Join::new(window, cx))),
                None,
            ),
            Err(error) => (
                Content::Message(format!("Could not open your terminals: {error}").into()),
                None,
            ),
        };
        let messages = messages.map(|mut messages| {
            cx.spawn(async move |this, cx| {
                while let Some(message) = messages.next().await {
                    if this
                        .update(cx, |view, cx| view.receive(message, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            })
        });
        let keys = Keys {
            tabs: cx.focus_handle().tab_stop(true),
            who: cx.focus_handle().tab_stop(true),
            new_terminal: cx.focus_handle().tab_stop(true),
            scroll: ScrollHandle::new(),
            mode: Mode::Pointer,
        };
        let input = Input::new(window, cx);
        if !matches!(content, Content::Join(_)) {
            window.focus(&input.focus);
        }
        let subscription = match &content {
            Content::Join(join) => Some(cx.subscribe_in(
                join,
                window,
                |view, _, joined: &crate::join::Joined, window, cx| {
                    view.joined(joined.0.clone(), window, cx)
                },
            )),
            _ => None,
        };
        Self {
            content,
            font,
            ui_font: "Inter".into(),
            keys,
            input,
            rows: Rows::default(),
            editing: None,
            menu: None,
            _messages: messages,
            _join: subscription,
            _tab_time: Some(tab_time::refresh(cx)),
        }
    }

    pub(crate) fn keys_mode_pointer(&mut self) {
        self.keys.mode = Mode::Pointer;
    }

    pub(crate) fn screen(&mut self) -> Option<(&Terminal, &mut Rows)> {
        match &self.content {
            Content::Terminal(link, _) => Some((link.terminal(), &mut self.rows)),
            Content::Message(_) | Content::Join(_) => None,
        }
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16, cx: &mut Context<Self>) {
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.resize(cols, rows),
            Content::Message(_) | Content::Join(_) => Ok(()),
        };
        self.check(result, cx);
    }

    pub(crate) fn select(&mut self, pane: &str, cx: &mut Context<Self>) {
        self.input.reset();
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.select(pane),
            Content::Message(_) | Content::Join(_) => Ok(()),
        };
        self.check(result, cx);
        cx.notify();
    }

    pub(crate) fn press(&mut self, pane: &str, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.keys.tabs);
        self.select(pane, cx);
    }

    pub(crate) fn new_terminal(&mut self, cx: &mut Context<Self>) {
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.new_terminal(),
            Content::Message(_) | Content::Join(_) => Ok(()),
        };
        self.check(result, cx);
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
        if matches!(self.content, Content::Join(_)) {
            if keystroke.key == "tab" {
                if keystroke.modifiers.shift {
                    window.focus_prev();
                } else {
                    window.focus_next();
                }
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        let held = keystroke.modifiers;
        let secondary = if cfg!(target_os = "macos") {
            held.platform
        } else {
            held.control
        };
        if secondary && held.shift && !held.alt && keystroke.key == "b" {
            self.focus_tabs(window, cx);
            cx.stop_propagation();
            return;
        }
        if self.input.focus.is_focused(window) || self.editing.is_some() || self.menu.is_some() {
            return;
        }
        self.keys.mode = match keystroke.key.as_str() {
            "tab" if keystroke.modifiers.shift => {
                window.focus_prev();
                Mode::Keyboard
            }
            "tab" if self.keys.tabs.is_focused(window) => {
                window.focus(&self.input.focus);
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
            Content::Terminal(link, _) => event.and_then(|event| link.receive(event)),
            Content::Message(_) | Content::Join(_) => Ok(()),
        };
        self.check(result, cx);
        cx.notify();
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

fn open() -> io::Result<Option<(Link, SharedString, Messages)>> {
    let store = ServerStore::load()?;
    let Some(server) = store.default_server() else {
        return Ok(None);
    };
    join::open_server(server).map(Some)
}

impl Render for SeerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // gpui starts a tip only on a mouse move that the tooltip listener
        // gets, so the tips come back in the frame after Escape clears them.
        if self.keys.mode == Mode::Quiet {
            cx.on_next_frame(window, |view, _, cx| {
                if view.keys.mode == Mode::Quiet {
                    view.keys.mode = Mode::Pointer;
                    cx.notify();
                }
            });
        }
        let root = div()
            .size_full()
            .relative()
            .flex()
            .bg(rgb(palette::WINDOW))
            .font_family(self.ui_font.clone())
            .text_size(px(13.))
            .line_height(relative(18. / 13.))
            .text_color(rgb(palette::TEXT))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.keys.mode = Mode::Pointer;
                    cx.notify();
                }),
            )
            .capture_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                view.travel(&event.keystroke, window, cx)
            }));
        let (link, name) = match &self.content {
            Content::Join(join) => return root.child(join.clone()),
            Content::Message(message) => {
                return root
                    .items_center()
                    .justify_center()
                    .p(px(24.))
                    .text_center()
                    .text_color(rgb(palette::FAINT))
                    .child(message.clone());
            }
            Content::Terminal(link, name) => (link, name),
        };
        let name: SharedString = chrome::capitalize(name).into();
        let labels = rename::Labels::new(self.editing.as_ref(), self.menu.as_ref());
        let terminals = link.terminals();
        let shown = link.shown();
        let terminal = self.terminal(Screen::new(cx.entity(), self.font.clone()), cx);
        if window.viewport_size().width < px(NARROW) {
            return root
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            chrome::bar()
                                .gap(px(8.))
                                .pl(px(14.))
                                .pr(px(8.))
                                .child(chrome::lights())
                                .child(
                                    sidebar::who(
                                        &name,
                                        20.,
                                        11.,
                                        window.viewport_size().width >= px(480.),
                                        &self.ui_font,
                                        &self.keys,
                                        window,
                                    )
                                    .min_w_0()
                                    .h(px(32.))
                                    .pl(px(5.))
                                    .pr(px(6.)),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(2.))
                                        .child(chrome::room(
                                            link.room(),
                                            true,
                                            window.viewport_size().width < px(480.),
                                        ))
                                        .child(sidebar::new_terminal(&self.keys, true, window, cx))
                                        .child(chrome::settings()),
                                ),
                        )
                        .when(!terminals.is_empty(), |main| {
                            main.child(sidebar::strip(
                                terminals, shown, labels, &self.keys, window, cx,
                            ))
                        })
                        .child(terminal),
                )
                .children(self.menu(window, cx));
        }
        let title = terminals
            .iter()
            .find(|terminal| Some(terminal.pane.as_str()) == shown)
            .cloned();
        let who = sidebar::who(&name, 22., 11., true, &self.ui_font, &self.keys, window);
        let side = sidebar::sidebar(who, terminals, shown, labels, &self.keys, window, cx)
            .child(chrome::footer(link.room()));
        let title = heading(title.as_ref(), labels, &self.ui_font, self.keys.hover(), cx);
        root.child(side)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(chrome::bar().justify_center().px(px(14.)).child(title))
                    .child(terminal),
            )
            .children(self.menu(window, cx))
    }
}

fn heading(
    terminal: Option<&seer_core::proto::TerminalInfo>,
    labels: rename::Labels<'_>,
    font: &SharedString,
    hover: bool,
    cx: &mut Context<SeerWindow>,
) -> Stateful<Div> {
    let title: SharedString = terminal
        .map(rename::display_name)
        .unwrap_or_default()
        .into();
    let pane = terminal.map(|terminal| terminal.pane.clone());
    tip::on_hover(
        div().id("title"),
        tip::terminal_help(&title),
        font.clone(),
        hover && !terminal.is_some_and(|terminal| labels.active(terminal)),
    )
    .max_w(px(520.))
    .h(px(28.))
    .px(px(8.))
    .rounded(px(6.))
    .flex()
    .items_center()
    .font_weight(FontWeight::MEDIUM)
    .on_mouse_down(
        MouseButton::Left,
        cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
            if event.click_count == 2
                && let Some(pane) = &pane
            {
                cx.stop_propagation();
                view.rename(pane, Surface::Title, window, cx);
            }
        }),
    )
    .children(terminal.map(|terminal| rename::label(terminal, Surface::Title, labels)))
}

#[cfg(test)]
mod input_e2e;

use std::io;

use futures::StreamExt;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, Context, Div, Font, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, Keystroke,
    MouseButton, ParentElement, Render, ScrollHandle, SharedString, Stateful, Styled, Task, Window,
    div, px, relative, rgb,
};
use seer::ServerStore;
use seer_core::TerminalFrame;
use seer_core::proto::ServerMsg;

use crate::link::{Link, Messages};
use crate::palette;
use crate::screen::{self, Screen};
use crate::sidebar::{self, Keys};
use crate::tip;

const NO_ROOM: &str = "To join a room, paste the line from the host into your terminal.";
const LINK_LOST: &str = "The link to your terminals stopped. Open Seer again.";
// The design's narrow layout starts below this window width.
const NARROW: f32 = 720.;
// gpui has no system UI font name on Linux. These follow the design font
// stack, then common Linux default sans fonts.
const UI_FAMILIES: [&str; 4] = ["Inter", "Noto Sans", "Cantarell", "DejaVu Sans"];

enum Content {
    Message(SharedString),
    Terminal(Box<Link>, SharedString),
}

pub(crate) struct SeerWindow {
    content: Content,
    font: Font,
    ui_font: SharedString,
    keys: Keys,
    _messages: Option<Task<()>>,
}

impl SeerWindow {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let font = screen::terminal_font(cx);
        let (content, messages) = match open() {
            Ok(Some((link, name, messages))) => {
                (Content::Terminal(Box::new(link), name), Some(messages))
            }
            Ok(None) => (Content::Message(NO_ROOM.into()), None),
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
            scroll: ScrollHandle::new(),
            ring: false,
        };
        window.focus(&keys.tabs);
        Self {
            content,
            font,
            ui_font: ui_font(cx),
            keys,
            _messages: messages,
        }
    }

    pub(crate) fn frame(&self) -> Option<&TerminalFrame> {
        match &self.content {
            Content::Terminal(link, _) => link.frame(),
            Content::Message(_) => None,
        }
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16, cx: &mut Context<Self>) {
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.resize(cols, rows),
            Content::Message(_) => Ok(()),
        };
        self.check(result, cx);
    }

    pub(crate) fn select(&mut self, pane: &str, cx: &mut Context<Self>) {
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.select(pane),
            Content::Message(_) => Ok(()),
        };
        self.check(result, cx);
        cx.notify();
    }

    pub(crate) fn press(&mut self, pane: &str, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.keys.tabs);
        self.select(pane, cx);
    }

    pub(crate) fn key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
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
        self.keys.ring = true;
        self.select(&pane, cx);
    }

    fn travel(&mut self, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        self.keys.ring = match keystroke.key.as_str() {
            "tab" if keystroke.modifiers.shift => {
                window.focus_prev();
                true
            }
            "tab" => {
                window.focus_next();
                true
            }
            "escape" => false,
            _ => return,
        };
        cx.notify();
    }

    fn receive(&mut self, message: io::Result<ServerMsg>, cx: &mut Context<Self>) {
        let result = match &mut self.content {
            Content::Terminal(link, _) => message.and_then(|message| link.receive(message)),
            Content::Message(_) => Ok(()),
        };
        self.check(result, cx);
        cx.notify();
    }

    fn check(&mut self, result: io::Result<()>, cx: &mut Context<Self>) {
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
    let (socket, tree, _notice) = seer::attach(server)?;
    let (link, messages) = Link::open(socket, &tree, server.user_id.clone())?;
    Ok(Some((link, server.name.clone().into(), messages)))
}

fn ui_font(cx: &App) -> SharedString {
    let system = ".SystemUIFont";
    if cfg!(target_os = "macos") {
        return system.into();
    }
    screen::first_installed(cx, &UI_FAMILIES)
        .unwrap_or(system)
        .into()
}

impl Render for SeerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .size_full()
            .flex()
            .bg(rgb(palette::WINDOW))
            .font_family(self.ui_font.clone())
            .text_size(px(13.))
            .line_height(relative(1.4))
            .text_color(rgb(palette::TEXT))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.keys.ring = false;
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                view.travel(&event.keystroke, window, cx)
            }));
        let (link, name) = match &self.content {
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
        let terminals = link.terminals();
        let shown = link.shown();
        let terminal = terminal(Screen::new(cx.entity(), self.font.clone()));
        if window.viewport_size().width < px(NARROW) {
            return root.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        bar().gap(px(10.)).pl(px(14.)).pr(px(8.)).child(
                            sidebar::who(name, 18., 10., &self.ui_font, &self.keys, window)
                                .min_w_0()
                                .h(px(32.))
                                .pl(px(6.))
                                .pr(px(8.)),
                        ),
                    )
                    .when(!terminals.is_empty(), |main| {
                        main.child(sidebar::strip(terminals, shown, &self.keys, window, cx))
                    })
                    .child(terminal),
            );
        }
        let title = terminals
            .iter()
            .find(|terminal| Some(terminal.pane.as_str()) == shown)
            .map(|terminal| terminal.name.clone());
        let who = sidebar::who(name, 22., 11., &self.ui_font, &self.keys, window);
        let side = sidebar::sidebar(who, terminals, shown, &self.keys, window, cx);
        let title = heading(name, title, &self.ui_font);
        root.child(side).child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(bar().justify_center().px(px(14.)).child(title))
                .child(terminal),
        )
    }
}

fn bar() -> Div {
    div()
        .flex_none()
        .h(px(44.))
        .flex()
        .items_center()
        .bg(rgb(palette::WINDOW))
        .border_b_1()
        .border_color(rgb(palette::LINE))
}

fn heading(name: &SharedString, terminal: Option<String>, font: &SharedString) -> Stateful<Div> {
    let full = match &terminal {
        Some(terminal) => format!("{name} / {terminal}").into(),
        None => name.clone(),
    };
    let title = tip::on_hover(div().id("title"), full, font.clone())
        .min_w_0()
        .flex()
        .font_weight(FontWeight::MEDIUM)
        .child(sidebar::one_line(div()).child(name.clone()));
    let Some(terminal) = terminal else {
        return title;
    };
    title
        .child(
            div()
                .flex_none()
                .mx(px(4.))
                .text_color(rgb(palette::FAINTER))
                .child("/"),
        )
        .child(
            sidebar::one_line(div())
                .font_weight(FontWeight::NORMAL)
                .text_color(rgb(palette::MUTED))
                .child(terminal),
        )
}

fn terminal(screen: Screen) -> Div {
    div()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .py(px(8.))
        .px(px(12.))
        .child(screen)
}

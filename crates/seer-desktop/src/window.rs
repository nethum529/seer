use std::io;

use futures::StreamExt;
use gpui::{
    Context, Font, IntoElement, ParentElement, Render, SharedString, Styled, Task, Window, div, px,
    rgb,
};
use seer::ServerStore;
use seer_core::TerminalFrame;
use seer_core::proto::ServerMsg;

use crate::link::{Link, Messages};
use crate::palette;
use crate::screen::{self, Screen};

const NO_ROOM: &str = "To join a room, paste the line from the host into your terminal.";
const LINK_LOST: &str = "The link to your terminals stopped. Open Seer again.";

enum Content {
    Message(SharedString),
    Terminal(Link),
}

pub(crate) struct SeerWindow {
    content: Content,
    font: Font,
    _messages: Option<Task<()>>,
}

impl SeerWindow {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let font = screen::terminal_font(cx);
        let (content, messages) = match open() {
            Ok(Some((link, messages))) => (Content::Terminal(link), Some(messages)),
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
        Self {
            content,
            font,
            _messages: messages,
        }
    }

    pub(crate) fn frame(&self) -> Option<&TerminalFrame> {
        match &self.content {
            Content::Terminal(link) => link.frame(),
            Content::Message(_) => None,
        }
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16, cx: &mut Context<Self>) {
        let result = match &mut self.content {
            Content::Terminal(link) => link.resize(cols, rows),
            Content::Message(_) => Ok(()),
        };
        self.check(result, cx);
    }

    fn receive(&mut self, message: io::Result<ServerMsg>, cx: &mut Context<Self>) {
        let result = match &mut self.content {
            Content::Terminal(link) => message.and_then(|message| link.receive(message)),
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

fn open() -> io::Result<Option<(Link, Messages)>> {
    let store = ServerStore::load()?;
    let Some(server) = store.default_server() else {
        return Ok(None);
    };
    let (socket, tree, _notice) = seer::attach(server)?;
    Link::open(socket, &tree, server.user_id.clone()).map(Some)
}

impl Render for SeerWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div().size_full().flex().flex_col().bg(rgb(palette::WINDOW));
        match &self.content {
            Content::Message(message) => root
                .items_center()
                .justify_center()
                .p(px(24.))
                .text_center()
                .text_size(px(13.))
                .text_color(rgb(palette::FAINT))
                .child(message.clone()),
            Content::Terminal(_) => root.pt(px(14.)).px(px(14.)).pb(px(12.)).child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(palette::LINE))
                    .bg(rgb(palette::CARD))
                    .py(px(8.))
                    .px(px(12.))
                    .child(Screen::new(cx.entity(), self.font.clone())),
            ),
        }
    }
}

use std::io;

use futures::StreamExt;
use gpui::{AppContext, Context, SharedString, Window};
use seer::ServerEntry;

use super::{Content, SeerWindow};
use crate::{
    chrome,
    join::{Join, Joined},
    link::{Link, Messages},
};

impl SeerWindow {
    pub(crate) fn open_invite(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(&self.content, Content::Join(join) if join.read(cx).joining()) {
            return;
        }
        let join = cx.new(|cx| Join::new(window, cx));
        join.update(cx, |join, cx| join.open_link(url, window, cx));
        self._join =
            Some(
                cx.subscribe_in(&join, window, |view, _, joined: &Joined, window, cx| {
                    view.joined(joined.0.clone(), window, cx);
                }),
            );
        self._messages = None;
        self.content = Content::Join(join);
        self.input.reset();
        window.activate_window();
        cx.activate(true);
        cx.notify();
    }

    pub(super) fn joined(
        &mut self,
        server: ServerEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.content = Content::Message("Opening your terminals...".into());
        self._join = None;
        let opened = cx
            .background_executor()
            .spawn(async move { open_server(&server) });
        self._messages = Some(cx.spawn_in(window, async move |view, cx| {
            let result = opened.await;
            let _ = view.update_in(cx, |view, window, cx| {
                match result {
                    Ok((link, name, mut messages)) => {
                        view.content = Content::Terminal(Box::new(link), name);
                        view._messages = Some(cx.spawn(async move |view, cx| {
                            while let Some(event) = messages.next().await {
                                if view.update(cx, |view, cx| view.receive(event, cx)).is_err() {
                                    break;
                                }
                            }
                        }));
                        window.focus(&view.input.focus);
                    }
                    Err(_) => {
                        view.content = Content::Message(
                            "Joined, but Seer could not open your terminals. Open Seer again."
                                .into(),
                        )
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

pub(super) fn open_server(server: &ServerEntry) -> io::Result<(Link, SharedString, Messages)> {
    let (socket, tree, _notice) = seer::attach(server)?;
    let metadata = serde_json::to_value(server).map_err(io::Error::other)?;
    let room = metadata
        .get("alias")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Room");
    let (link, messages) = Link::open(
        socket,
        &tree,
        server.user_id.clone(),
        chrome::capitalize(room),
    )?;
    Ok((link, server.name.clone().into(), messages))
}

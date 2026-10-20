use std::io;

use gpui::{AppContext, Context, Window};
use seer::ServerEntry;

use super::{Content, SeerWindow, room::RoomUi};
use crate::{
    chrome,
    join::{Event, Join},
    link::{Link, Messages},
};

impl SeerWindow {
    pub(crate) fn join_open(&self) -> bool {
        self.join_screen.is_some()
    }

    pub(super) fn join_travel(
        &self,
        key: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.join_open() {
            return false;
        }
        if key.key == "escape" {
            if let Some(join) = &self.join_screen {
                join.update(cx, |join, cx| join.go_back(window, cx));
            }
            cx.stop_propagation();
        }
        if key.key == "tab" {
            if key.modifiers.shift {
                window.focus_prev();
            } else {
                window.focus_next();
            }
            cx.stop_propagation();
            cx.notify();
        }
        true
    }

    pub(crate) fn open_invite(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_join(Some(url), window, cx);
    }

    pub(super) fn open_join(
        &mut self,
        url: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .join_screen
            .as_ref()
            .is_some_and(|join| join.read(cx).joining())
        {
            return;
        }
        let join = cx.new(|cx| Join::new(window, cx));
        if let Some(url) = url {
            join.update(cx, |join, cx| join.open_link(url, window, cx));
        }
        self._join =
            Some(cx.subscribe_in(
                &join,
                window,
                |view, _, event: &Event, window, cx| match event {
                    Event::Joined(server) => view.joined(server.clone(), window, cx),
                    Event::Back => view.close_join(window, cx),
                },
            ));
        self.invite = None;
        self.join_screen = Some(join);
        self.people_menu = None;
        self.menu = None;
        self.editing = None;
        self.close_room_menu(true, false, window, cx);
        self.input.reset();
        window.activate_window();
        cx.activate(true);
        cx.notify();
    }

    pub(super) fn close_join(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.join_screen = None;
        self._join = None;
        match &self.content {
            Content::Start(start) => window.focus(&start.host),
            Content::Terminal(..) => window.focus(&self.input.focus),
            Content::Message(_) => window.focus(&self.room.focus),
        }
        cx.notify();
    }

    pub(super) fn joined(
        &mut self,
        server: ServerEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.join_screen = None;
        self.input.reset();
        self.rows = super::Rows::default();
        self.content = Content::Message("Opening your terminals...".into());
        self.server = Some(server.clone());
        self._join = None;
        self.people_menu = None;
        self.room = RoomUi::new(Some(server.clone()), cx);
        self.refresh_room(cx);
        let opened = cx
            .background_executor()
            .spawn(async move { open_server(&server) });
        self._messages = Some(cx.spawn_in(window, async move |view, cx| {
            let result = opened.await;
            let _ = view.update_in(cx, |view, window, cx| {
                match result {
                    Ok((link, server, messages)) => {
                        view.content =
                            Content::Terminal(Box::new(link), server.name.clone().into());
                        view.server = Some(server);
                        view.listen(messages, cx);
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

pub(super) fn open_server(server: &ServerEntry) -> io::Result<(Link, ServerEntry, Messages)> {
    let (socket, tree, _notice) = seer::attach(server)?;
    let (mut link, messages) = Link::open(
        socket,
        &tree,
        server.user_id.clone(),
        chrome::capitalize(&server.alias),
    )?;
    let room_messages = link.join_room(
        server.clone(),
        std::env::args().skip_while(|arg| arg != "--peek").nth(1),
    );
    Ok((
        link,
        server.clone(),
        Box::pin(futures::stream::select(messages, room_messages)),
    ))
}

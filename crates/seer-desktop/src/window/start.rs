use gpui::{Bounds, FocusHandle, Pixels, StatefulInteractiveElement};
use std::{cell::RefCell, rc::Rc};

use super::*;

pub(crate) struct StartScreen {
    pub(super) host: FocusHandle,
    join: FocusHandle,
    pub(super) saved: Vec<(ServerEntry, FocusHandle)>,
    scroll: ScrollHandle,
    bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
}

impl StartScreen {
    pub(super) fn new(cx: &mut Context<SeerWindow>) -> Self {
        let store = ServerStore::load().unwrap_or_default();
        let mut servers = store.servers().to_vec();
        servers.sort_by_key(|server| Some(server) != store.default_server());
        Self {
            host: cx.focus_handle().tab_stop(true),
            join: cx.focus_handle().tab_stop(true),
            scroll: ScrollHandle::new(),
            bounds: Rc::default(),
            saved: servers
                .into_iter()
                .map(|server| (server, cx.focus_handle().tab_stop(true)))
                .collect(),
        }
    }
}

impl SeerWindow {
    pub(super) fn start_screen(
        &self,
        start: &StartScreen,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let actions = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                crate::join::button(
                    "start-room",
                    "Start a room",
                    &start.host,
                    true,
                    false,
                    window,
                )
                .on_click(cx.listener(|view, _, window, cx| view.start_room(window, cx))),
            )
            .child(
                crate::join::button(
                    "start-join",
                    "Join a room",
                    &start.join,
                    false,
                    false,
                    window,
                )
                .on_click(cx.listener(|view, _, window, cx| view.open_join(None, window, cx))),
            );
        let bounds = start.bounds.clone();
        let scroll = start.scroll.clone();
        crate::join::layout(
            "Seer",
            div().child(actions).when(!start.saved.is_empty(), |body| {
                body.child(
                    div()
                        .mt(px(24.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .on_children_prepainted(move |rows, _, _| {
                            *bounds.borrow_mut() =
                                rows.into_iter().map(|row| row - scroll.offset()).collect();
                        })
                        .children(start.saved.iter().map(|(server, focus)| {
                            let server = server.clone();
                            let up = self.room.others.iter().any(|room| {
                                room.server.user_id == server.user_id
                                    && room.state != seer::rooms::RoomState::Unreachable
                            });
                            room::saved_row(&server, up)
                                .relative()
                                .track_focus(focus)
                                .hover(|row| row.bg(rgb(palette::HOVER)))
                                .when(focus.is_focused(window), |row| {
                                    row.child(sidebar::outline())
                                })
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.pick_saved_room(server.clone(), window, cx)
                                }))
                        })),
                )
            }),
            Some(&start.scroll),
            window,
            cx,
        )
        .when(self.room.modal(), |screen| screen.invisible())
    }

    pub(super) fn start_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Content::Start(start) = &self.content else {
            return false;
        };
        if key.key == "tab" {
            if key.modifiers.shift {
                window.focus_prev();
            } else {
                window.focus_next();
            }
            if let Some(at) = start
                .saved
                .iter()
                .position(|(_, focus)| focus.is_focused(window))
            {
                if let Some(bounds) = start.bounds.borrow().get(at) {
                    sidebar::reveal(&start.scroll, *bounds, px(0.));
                }
            } else if start.host.is_focused(window) || start.join.is_focused(window) {
                start.scroll.set_offset(gpui::Point::default());
            }
            self.keys.mode = Mode::Keyboard;
            cx.stop_propagation();
            cx.notify();
            return true;
        }
        if matches!(key.key.as_str(), "enter" | "space") {
            if start.host.is_focused(window) {
                self.start_room(window, cx);
            } else if start.join.is_focused(window) {
                self.open_join(None, window, cx);
            } else if let Some((server, _)) = start
                .saved
                .iter()
                .find(|(_, focus)| focus.is_focused(window))
            {
                self.pick_saved_room(server.clone(), window, cx);
            } else {
                return false;
            }
            cx.stop_propagation();
            return true;
        }
        false
    }

    pub(super) fn pick_saved_room(
        &mut self,
        server: ServerEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(stopped) = seer::rooms::stopped_host(&server) {
            self.content = Content::Message(room::STOPPED.into());
            self.room = room::RoomUi::new(Some(server.clone()), cx);
            self.room.current = Some(stopped);
            self.server = Some(server);
            window.focus(&self.room.focus);
            cx.notify();
        } else {
            self.joined(server, window, cx);
        }
    }
}

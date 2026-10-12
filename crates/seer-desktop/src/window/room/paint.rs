use gpui::{AnyElement, StatefulInteractiveElement, canvas};

use super::*;
use crate::cap_icon::cap_icon;
use crate::menu;

impl SeerWindow {
    pub(crate) fn room_button(
        &self,
        name: &str,
        narrow: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let bounds = self.room.anchor.clone();
        let up = self
            .room
            .current
            .as_ref()
            .is_some_and(|room| room.state != RoomState::Unreachable);
        let ring = self.keys.ring(&self.room.focus, window);
        chrome::room(
            name,
            narrow,
            narrow && window.viewport_size().width < px(480.),
            up,
        )
        .id("room-trigger")
        .debug_selector(|| "room-trigger".into())
        .relative()
        .track_focus(&self.room.focus)
        .rounded(px(8.))
        .when(ring, |row| row.child(sidebar::outline()))
        .when(self.room.open(), |row| {
            row.bg(rgb(palette::SELECTED))
                .text_color(rgb(palette::TEXT))
        })
        .hover(|row| row.bg(rgb(palette::HOVER)))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|view, event, window, cx| {
            if !matches!(event, gpui::ClickEvent::Mouse(_)) {
                return;
            }
            if view.room.open() {
                view.close_room_menu(false, true, window, cx);
            } else {
                view.open_room(false, window, cx);
            }
        }))
        .child(
            canvas(move |rect, _, _| bounds.set(rect), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
    }

    pub(in crate::window) fn room_overlays(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.room_tick(window, cx);
        let mut overlays = Vec::new();
        if let Some(menu) = self.room_menu(window, cx) {
            overlays.push(menu);
        }
        if let Some(sheet) = self.room_sheet(window, cx) {
            overlays.push(sheet);
        }
        overlays
    }

    fn room_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.room.menu.as_ref()?;
        let viewport = window.viewport_size();
        let anchor = self.room.anchor.get();
        let width = 280_f32.min(f32::from(viewport.width) - 16.);
        let left = anchor
            .left()
            .clamp(px(8.), (viewport.width - px(width) - px(8.)).max(px(8.)));
        let height = viewport.height - px(68.);
        let up = anchor.top() > viewport.height / 2.;
        let panel = menu::panel("room-menu", width)
            .debug_selector(|| "room-menu".into())
            .absolute()
            .left(left)
            .when(up, |menu| {
                menu.bottom(viewport.height - anchor.top() + px(4.))
            })
            .when(!up, |menu| menu.top(anchor.bottom() + px(4.)))
            .max_h(height)
            .overflow_y_scroll()
            .track_focus(&state.focus)
            .opacity(state.motion.value())
            .on_mouse_down_out(
                cx.listener(|view, _, window, cx| view.close_room_menu(false, false, window, cx)),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(self.room.current.as_ref().map(|room| room_row(room, true)))
            .when(self.room.current.is_none(), |menu| {
                menu.child(menu::item("room-loading", 36.).child("Checking room..."))
            })
            .children(self.room.others.iter().map(|room| room_row(room, false)))
            .child(menu::separator())
            .children(
                self.room
                    .actions()
                    .into_iter()
                    .enumerate()
                    .map(|(index, action)| {
                        let selected = state.selected == Some(index);
                        menu::item(("room-action", index), 36.)
                            .debug_selector(move || action_id(action).into())
                            .text_color(rgb(action_color(action)))
                            .when(selected, |item| {
                                item.bg(rgb(if action == RoomAction::Restore {
                                    palette::CHIP
                                } else {
                                    palette::DANGER_HOVER
                                }))
                            })
                            .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                                if let Some(menu) = &mut view.room.menu {
                                    menu.selected = hovered.then_some(index);
                                    view.notify_room(cx);
                                }
                            }))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.pick_room_action(action, false, window, cx)
                            }))
                            .child(cap_icon(div().w(px(22.)).flex().justify_center().child(
                                chrome::icon(action_icon(action), action_color(action)),
                            )))
                            .child(action_label(action))
                    }),
            )
            .children(self.room.error.as_ref().map(|error| {
                div()
                    .p(px(8.))
                    .text_size(px(12.))
                    .text_color(rgb(palette::DANGER))
                    .child(error.clone())
            }));
        Some(panel.into_any_element())
    }

    pub(in crate::window) fn empty_room(
        &self,
        message: &SharedString,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let name = self
            .room
            .current
            .as_ref()
            .map(RoomSummary::room_name)
            .or_else(|| {
                self.room
                    .server
                    .as_ref()
                    .map(|server| server.alias.as_str())
            })
            .unwrap_or("Room");
        let name = chrome::capitalize(name);
        let person: SharedString = self
            .room
            .server
            .as_ref()
            .map(|server| chrome::capitalize(&server.name).into())
            .unwrap_or_default();
        let narrow = window.viewport_size().width < px(NARROW);
        let button = self.room_button(&name, narrow, window, cx);
        let main = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                chrome::bar()
                    .gap(px(8.))
                    .pl(px(14.))
                    .pr(px(8.))
                    .when(narrow, |bar| {
                        bar.child(chrome::lights())
                            .child(
                                sidebar::who(
                                    &person,
                                    (20., 11.),
                                    window.viewport_size().width >= px(480.),
                                    &self.ui_font,
                                    &self.keys,
                                    window,
                                    false,
                                )
                                .min_w_0()
                                .h(px(32.))
                                .pl(px(5.))
                                .pr(px(6.)),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(2.))
                                    .child(self.room_button(&name, true, window, cx))
                                    .child(chrome::settings()),
                            )
                    })
                    .when(!narrow, |bar| {
                        bar.child(
                            div()
                                .flex_1()
                                .text_center()
                                .font_weight(FontWeight::MEDIUM)
                                .child(person.clone()),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(24.))
                    .text_center()
                    .text_color(rgb(palette::FAINT))
                    .child(if message.as_ref() == STOPPED {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_color(rgb(palette::TEXT_2))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("The room is stopped"),
                            )
                            .child("Open the room menu to restore it.")
                    } else {
                        div().child(message.clone())
                    }),
            );
        if narrow {
            return main;
        }
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .w(px(248.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .px(px(8.))
                    .bg(rgb(palette::SURFACE))
                    .border_r_1()
                    .border_color(rgb(palette::LINE))
                    .child(
                        div()
                            .h(px(44.))
                            .flex_none()
                            .pl(px(12.))
                            .child(chrome::lights()),
                    )
                    .child(
                        sidebar::who(
                            &person,
                            (22., 11.),
                            true,
                            &self.ui_font,
                            &self.keys,
                            window,
                            false,
                        )
                        .w_full()
                        .h(px(36.))
                        .pl(px(7.))
                        .pr(px(8.)),
                    )
                    .child(div().flex_1())
                    .child(chrome::footer(button)),
            )
            .child(main)
    }
}

fn room_row(room: &RoomSummary, current: bool) -> Stateful<Div> {
    let up = room.state != RoomState::Unreachable;
    let status = if current && up {
        format!("{} people", room.people.len())
    } else if current && room.host {
        "Stopped".into()
    } else {
        chrome::capitalize(room.state.label())
    };
    menu::item(
        gpui::SharedString::from(format!("saved-room-{}", room.server.user_id)),
        36.,
    )
    .child(cap_icon(
        div()
            .w(px(22.))
            .flex_none()
            .flex()
            .justify_center()
            .child(chrome::room_dot(up)),
    ))
    .child(
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_color(rgb(if current {
                palette::TEXT
            } else {
                palette::TEXT_2
            }))
            .child(
                sidebar::one_line(div())
                    .flex_none()
                    .max_w(px(112.))
                    .child(chrome::capitalize(room.room_name())),
            )
            .when(current, |row| {
                row.child(
                    sidebar::one_line(div())
                        .text_size(px(12.))
                        .font_weight(FontWeight::NORMAL)
                        .font_family("JetBrains Mono")
                        .text_color(rgb(palette::FAINT))
                        .child(room.address().to_owned()),
                )
            }),
    )
    .child(
        div()
            .flex_none()
            .text_size(px(12.))
            .font_weight(FontWeight::NORMAL)
            .text_color(rgb(palette::FAINT))
            .child(status),
    )
    .child(cap_icon(
        div().w(px(16.)).flex_none().when(current, |slot| {
            slot.child(chrome::icon("check", palette::TEXT))
        }),
    ))
}

pub(super) fn action_label(action: RoomAction) -> &'static str {
    match action {
        RoomAction::Stop => "Stop room",
        RoomAction::Leave => "Leave room",
        RoomAction::Restore => "Restore room",
        RoomAction::StartNew => "Start a new room",
    }
}

fn action_icon(action: RoomAction) -> &'static str {
    match action {
        RoomAction::Stop => "stop",
        RoomAction::Leave => "leave",
        RoomAction::Restore => "restore",
        RoomAction::StartNew => "fresh",
    }
}

fn action_id(action: RoomAction) -> &'static str {
    match action {
        RoomAction::Stop => "room-stop",
        RoomAction::Leave => "room-leave",
        RoomAction::Restore => "room-restore",
        RoomAction::StartNew => "room-start-new",
    }
}

fn action_color(action: RoomAction) -> u32 {
    if action == RoomAction::Restore {
        palette::TEXT_2
    } else {
        palette::DANGER
    }
}

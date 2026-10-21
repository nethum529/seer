use gpui::{AnyElement, FocusHandle, ScrollHandle, StatefulInteractiveElement};
use seer_core::proto::{Person, PersonState};

use super::*;
use crate::{menu, motion::Motion};

mod remove;

pub(super) struct Menu {
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    scroll: ScrollHandle,
    pub(super) highlighted: String,
    removing: bool,
    pub(super) motion: Motion,
}

impl SeerWindow {
    pub(super) fn open_people(
        &mut self,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.room.modal() || self.room.open() {
            return;
        }
        self.finish_name(true, false, window, cx);
        self.menu = None;
        let Content::Terminal(link, _) = &self.content else {
            return;
        };
        if let Some(menu) = &mut self.people_menu {
            if !keyboard && !menu.motion.closing() {
                self.close_people(keyboard, false, window, cx);
                return;
            }
            if menu.motion.closing() {
                menu.return_focus = window.focused(cx);
            }
            menu.motion.reopen(keyboard, 150);
            window.focus(&menu.focus);
        } else {
            let return_focus = window.focused(cx);
            let focus = cx.focus_handle();
            window.focus(&focus);
            self.people_menu = Some(Menu {
                focus,
                return_focus,
                scroll: ScrollHandle::new(),
                highlighted: link.people.selected.clone(),
                removing: false,
                motion: Motion::open(keyboard, 150),
            });
        }
        if keyboard
            && let Some(menu) = &mut self.people_menu
            && let Some(first) = link.people.people.first()
        {
            menu.highlighted.clone_from(&first.user_id);
            menu.removing = false;
            menu.scroll.scroll_to_item(0);
        }
        self.keys.mode = if keyboard {
            Mode::Keyboard
        } else {
            Mode::Pointer
        };
        self.notify_room(cx);
    }

    fn close_people(
        &mut self,
        keyboard: bool,
        picked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(menu) = &mut self.people_menu {
            menu.motion.close(keyboard, 80);
        }
        let previous = self
            .people_menu
            .as_ref()
            .and_then(|menu| menu.return_focus.as_ref())
            .unwrap_or(&self.keys.who);
        window.focus(if picked { &self.input.focus } else { previous });
        self.keys.mode = if keyboard { Mode::Quiet } else { Mode::Pointer };
        self.notify_room(cx);
    }

    fn choose_person(
        &mut self,
        user: &str,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.select_person(user),
            Content::Message(_) | Content::Start(_) => Ok(()),
        };
        self.input.reset();
        self.rows = Rows::default();
        self.check(result, cx);
        self.close_people(keyboard, true, window, cx);
    }

    pub(super) fn people_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .people_menu
            .as_ref()
            .is_none_or(|menu| menu.motion.closing())
        {
            if self.keys.who.is_focused(window)
                && matches!(key.key.as_str(), "enter" | "space" | "down" | "up")
            {
                self.open_people(true, window, cx);
                return true;
            }
            return false;
        }
        if matches!(key.key.as_str(), "escape" | "tab") {
            self.close_people(true, false, window, cx);
            return true;
        }
        let (Some(menu), Content::Terminal(link, _)) = (&mut self.people_menu, &self.content)
        else {
            return true;
        };
        if matches!(key.key.as_str(), "enter" | "space") {
            let user = menu.highlighted.clone();
            if menu.removing {
                self.confirm_remove(&user, true, window, cx);
            } else if key.key == "space"
                && !link
                    .people
                    .people
                    .first()
                    .is_some_and(|person| person.user_id == user)
            {
                self.share_control(&user, cx);
            } else {
                self.choose_person(&user, true, window, cx);
            }
        } else {
            self.step_person(&key.key, cx);
        }
        true
    }

    pub(super) fn people_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self
            .people_menu
            .as_ref()
            .is_some_and(|menu| menu.motion.gone())
        {
            self.people_menu = None;
        }
        let menu = self.people_menu.as_ref()?;
        if menu.motion.animating() {
            cx.on_next_frame(window, |view, _, cx| view.notify_room(cx));
        }
        let Content::Terminal(link, _) = &self.content else {
            return None;
        };
        let narrow = window.viewport_size().width < px(NARROW);
        let left = if narrow { 74. } else { 8. };
        let top = if narrow { 48. } else { 92. };
        let left = px(left).min((window.viewport_size().width - px(288.)).max(px(8.)));
        let panel = menu::panel("people-menu", 280., cx)
            .absolute()
            .left(left)
            .top(px(top))
            .opacity(menu.motion.value())
            .track_focus(&menu.focus)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("people-list")
                    .max_h((window.viewport_size().height - px(top + 18.)).max(px(36.)))
                    .overflow_y_scroll()
                    .track_scroll(&menu.scroll)
                    .children(link.people.people.iter().enumerate().map(|(at, person)| {
                        div()
                            .flex()
                            .flex_col()
                            .child(person_row(
                                person,
                                at == 0,
                                person.user_id == link.people.selected,
                                person.user_id == menu.highlighted && !menu.removing,
                                link.people.activity(&person.user_id),
                                cx,
                            ))
                            .when(at != 0, |row| {
                                row.child(share_switch(
                                    person,
                                    &link.people,
                                    menu.highlighted == person.user_id
                                        && !menu.removing
                                        && self.keys.mode == Mode::Keyboard,
                                    cx,
                                ))
                            })
                            .when(link.people.can_remove(&person.user_id), |row| {
                                row.child(remove::item(
                                    person,
                                    menu.highlighted == person.user_id && menu.removing,
                                    cx,
                                ))
                            })
                    })),
            )
            .children(link.people.control_error.map(|error| {
                div()
                    .debug_selector(move || error.into())
                    .p(px(8.))
                    .text_size(px(12.))
                    .text_color(rgb(palette::DANGER))
                    .child(crate::label::text(error))
            }));
        let closing = menu.motion.closing();
        Some(
            div()
                .absolute()
                .inset_0()
                .when(!closing, |layer| {
                    layer.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, window, cx| {
                            cx.stop_propagation();
                            view.close_people(false, false, window, cx);
                        }),
                    )
                })
                .child(panel)
                .into_any_element(),
        )
    }
}

fn person_row(
    person: &Person,
    own: bool,
    selected: bool,
    highlighted: bool,
    activity: (bool, usize),
    cx: &mut Context<SeerWindow>,
) -> Stateful<Div> {
    let user = person.user_id.clone();
    let hover_user = user.clone();
    let name = chrome::capitalize(&person.name);
    let tag = tags(&name, own, person.host);
    let offline = !person.online;
    let waiting = activity.0;
    let away = person.state == PersonState::Away;
    let text = if offline {
        palette::FAINT
    } else if selected || highlighted {
        palette::TEXT
    } else {
        palette::TEXT_2
    };
    menu::item(SharedString::from(format!("person-{user}")), 36.)
        .flex_none()
        .when(highlighted, |row| row.bg(rgb(palette::CHIP)))
        .on_mouse_move(cx.listener(move |view, _, _, cx| {
            if let Some(menu) = &mut view.people_menu
                && (menu.highlighted != hover_user || menu.removing)
            {
                menu.highlighted.clone_from(&hover_user);
                menu.removing = false;
                view.keys.mode = Mode::Pointer;
                view.notify_room(cx);
            }
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, window, cx| {
                cx.stop_propagation();
                view.choose_person(&user, false, window, cx);
            }),
        )
        .child(sidebar::avatar(&name, 22., 11.).opacity(if offline {
            0.4
        } else if away {
            0.6
        } else {
            1.
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    sidebar::one_line(div())
                        .text_color(rgb(text))
                        .opacity(if away && !offline { 0.6 } else { 1. })
                        .child(crate::label::text(name.clone())),
                )
                .when(!tag.is_empty(), |row| {
                    let label = format!("{name}: {tag}");
                    row.child(mark(tag).debug_selector(move || label))
                }),
        )
        .child(
            mark(presence(person, activity)).text_color(rgb(if waiting && !offline {
                palette::WAITING
            } else {
                palette::FAINT
            })),
        )
        .child(div().flex_none().size(px(16.)).when(selected, |slot| {
            slot.child(chrome::icon("check", palette::TEXT))
        }))
}

fn tags(name: &str, own: bool, host: bool) -> String {
    [(own, "You"), (host, "Host")]
        .into_iter()
        .filter(|&(on, word)| on && !word.eq_ignore_ascii_case(name))
        .map(|(_, word)| word)
        .collect::<Vec<_>>()
        .join(", ")
}

fn mark(text: impl Into<SharedString>) -> Div {
    div()
        .flex_none()
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(FontWeight::NORMAL)
        .text_color(rgb(palette::FAINT))
        .whitespace_nowrap()
        .child(crate::label::text(text))
}

fn presence(person: &Person, (waiting, working): (bool, usize)) -> String {
    if !person.online {
        return format!("Offline {}", elapsed(person.idle_secs));
    }
    if waiting {
        return "Waiting".into();
    }
    if person.state == PersonState::Away {
        return format!("Away {}", elapsed(person.idle_secs));
    }
    if working > 0 {
        return format!("{working} working");
    }
    String::new()
}

fn elapsed(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}m", seconds / 60)
    }
}

pub(super) fn control(can_input: bool) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(FontWeight::NORMAL)
        .text_color(rgb(palette::FAINT))
        .debug_selector(move || {
            if can_input {
                "You have control"
            } else {
                "Read only"
            }
            .into()
        })
        .child(chrome::icon(if can_input { "check" } else { "lock" }, palette::FAINT).size(px(12.)))
        .child(crate::label::text(if can_input {
            "You have control"
        } else {
            "Read only"
        }))
}

impl SeerWindow {
    fn share_control(&mut self, user: &str, cx: &mut Context<Self>) {
        if let Content::Terminal(link, _) = &mut self.content {
            link.people.share_control(user);
        }
        self.notify_room(cx);
    }
}

fn share_switch(
    person: &Person,
    room: &crate::room::Room,
    keyboard: bool,
    cx: &mut Context<SeerWindow>,
) -> Stateful<Div> {
    let user = person.user_id.clone();
    let enabled = room.can_share();
    let on = room.shares_control(&user);
    let label = format!(
        "Share control for {}: {}",
        chrome::capitalize(&person.name),
        if on { "On" } else { "Off" }
    );
    menu::item(SharedString::from(format!("share-control-{user}")), 36.)
        .debug_selector(move || label.clone())
        .pl(px(40.))
        .font_weight(FontWeight::NORMAL)
        .text_size(px(12.))
        .text_color(rgb(palette::FAINT))
        .opacity(if enabled { 1. } else { 0.5 })
        .when(enabled, |row| {
            row.cursor_pointer().hover(|row| row.bg(rgb(palette::CHIP)))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, _, cx| {
                cx.stop_propagation();
                if let Some(menu) = &mut view.people_menu {
                    menu.highlighted.clone_from(&user);
                    menu.removing = false;
                }
                view.share_control(&user, cx);
            }),
        )
        .child(crate::label::text("Share control"))
        .child(div().flex_1())
        .when(keyboard, |row| row.child(crate::label::text("Space")))
        .child(menu::switch_track(on))
}

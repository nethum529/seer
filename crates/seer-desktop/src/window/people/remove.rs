use super::*;
use crate::cap_icon::cap_icon;

impl SeerWindow {
    pub(super) fn step_person(&mut self, key: &str, cx: &mut Context<Self>) {
        let (Some(menu), Content::Terminal(link, _)) = (&mut self.people_menu, &self.content)
        else {
            return;
        };
        let choices: Vec<_> = link
            .people
            .people
            .iter()
            .flat_map(|person| {
                std::iter::once((&person.user_id, false)).chain(
                    link.people
                        .can_remove(&person.user_id)
                        .then_some((&person.user_id, true)),
                )
            })
            .collect();
        let now = choices
            .iter()
            .position(|(user, removing)| **user == menu.highlighted && *removing == menu.removing);
        if let Some(at) = sidebar::step(key, choices.len(), now) {
            menu.highlighted.clone_from(choices[at].0);
            menu.removing = choices[at].1;
            menu.motion.reopen(true, 0);
            if let Some(at) = link
                .people
                .people
                .iter()
                .position(|person| person.user_id == menu.highlighted)
            {
                menu.scroll.scroll_to_item(at);
            }
            self.keys.mode = Mode::Keyboard;
            self.notify_room(cx);
        }
    }
}

pub(super) fn item(
    person: &Person,
    highlighted: bool,
    cx: &mut Context<SeerWindow>,
) -> Stateful<Div> {
    let user = person.user_id.clone();
    let hover_user = user.clone();
    let label = format!("Remove {}", chrome::capitalize(&person.name));
    menu::item(SharedString::from(format!("remove-person-{user}")), 36.)
        .debug_selector(move || label.clone())
        .text_color(rgb(palette::DANGER))
        .cursor_pointer()
        .when(highlighted, |row| row.bg(rgb(palette::DANGER_HOVER)))
        .hover(|row| row.bg(rgb(palette::DANGER_HOVER)))
        .on_mouse_move(cx.listener(move |view, _, _, cx| {
            if let Some(menu) = &mut view.people_menu
                && (menu.highlighted != hover_user || !menu.removing)
            {
                menu.highlighted.clone_from(&hover_user);
                menu.removing = true;
                view.keys.mode = Mode::Pointer;
                view.notify_room(cx);
            }
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, window, cx| {
                cx.stop_propagation();
                view.confirm_remove(&user, false, window, cx);
            }),
        )
        .child(cap_icon(
            div()
                .w(px(22.))
                .flex()
                .justify_center()
                .child(chrome::icon("leave", palette::DANGER)),
        ))
        .child(crate::label::text("Remove"))
}

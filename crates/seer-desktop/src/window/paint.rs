use super::*;

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
                let content = if self.room.server.is_some() {
                    self.empty_room(message, window, cx)
                } else {
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .p(px(24.))
                        .text_center()
                        .text_color(rgb(palette::FAINT))
                        .child(message.clone())
                };
                return root.child(content).children(self.room_overlays(window, cx));
            }
            Content::Terminal(link, name) => (link, name),
        };
        let name: SharedString = chrome::capitalize(
            link.people
                .person()
                .map_or(name.as_ref(), |person| &person.name),
        )
        .into();
        let readonly = link.readonly();
        let person_open = self
            .people_menu
            .as_ref()
            .is_some_and(|menu| !menu.motion.closing());
        let labels = rename::Labels::new(self.editing.as_ref(), self.menu.as_ref(), readonly);
        let terminals = link.terminals();
        let shown = link.shown();
        let terminal = if readonly && shown.is_none() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(palette::FAINT))
                .child(
                    if link.people.person().is_some_and(|person| person.peekable) {
                        "No terminals"
                    } else {
                        "Offline"
                    },
                )
                .into_any_element()
        } else {
            self.terminal(Screen::new(cx.entity(), self.font.clone()), cx)
                .into_any_element()
        };
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
                                        (20., 11.),
                                        window.viewport_size().width >= px(480.),
                                        &self.ui_font,
                                        &self.keys,
                                        window,
                                        person_open,
                                    )
                                    .when(person_open, |who| who.bg(rgb(palette::SELECTED)))
                                    .min_w_0()
                                    .h(px(32.))
                                    .pl(px(5.))
                                    .pr(px(6.))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|view, _, window, cx| {
                                            cx.stop_propagation();
                                            view.open_people(false, window, cx);
                                        }),
                                    ),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(2.))
                                        .child(self.room_button(link.room(), true, window, cx))
                                        .when(!readonly, |bar| {
                                            bar.child(sidebar::new_terminal(
                                                &self.keys, true, window, cx,
                                            ))
                                        })
                                        .when(
                                            readonly && window.viewport_size().width >= px(480.),
                                            |bar| bar.child(people::read_only()),
                                        )
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
                .children(self.menu(window, cx))
                .children(self.people_menu(window, cx))
                .children(self.room_overlays(window, cx));
        }
        let title = terminals
            .iter()
            .find(|terminal| Some(terminal.pane.as_str()) == shown)
            .cloned();
        let who = sidebar::who(
            &name,
            (22., 11.),
            true,
            &self.ui_font,
            &self.keys,
            window,
            person_open,
        )
        .when(person_open, |who| who.bg(rgb(palette::SELECTED)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|view, _, window, cx| {
                cx.stop_propagation();
                view.open_people(false, window, cx);
            }),
        );
        let side = sidebar::sidebar(who, terminals, shown, labels, &self.keys, window, cx).child(
            chrome::footer(self.room_button(link.room(), false, window, cx)),
        );
        let title = heading(
            title.as_ref(),
            labels,
            readonly.then_some(&name),
            &self.ui_font,
            self.keys.hover(),
            cx,
        );
        let heading = chrome::bar()
            .relative()
            .justify_center()
            .px(px(14.))
            .child(
                title.max_w(
                    (window.viewport_size().width - px(248. + if readonly { 180. } else { 28. }))
                        .max(px(0.)),
                ),
            )
            .when(readonly, |bar| {
                bar.child(people::read_only().absolute().right(px(14.)).top(px(13.)))
            });
        root.child(side)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(heading)
                    .child(terminal),
            )
            .children(self.menu(window, cx))
            .children(self.people_menu(window, cx))
            .children(self.room_overlays(window, cx))
    }
}

fn heading(
    terminal: Option<&seer_core::proto::TerminalInfo>,
    labels: rename::Labels<'_>,
    person: Option<&SharedString>,
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
        hover && !labels.readonly && !terminal.is_some_and(|terminal| labels.active(terminal)),
    )
    .max_w(px(520.))
    .h(px(28.))
    .px(px(8.))
    .rounded(px(6.))
    .flex()
    .items_center()
    .font_weight(FontWeight::MEDIUM)
    .gap(px(6.))
    .when_some(person, |title, name| {
        title
            .child(
                sidebar::one_line(div())
                    .font_weight(FontWeight::NORMAL)
                    .text_color(rgb(palette::MUTED))
                    .child(name.clone()),
            )
            .when(terminal.is_some(), |title| {
                title.child(
                    div()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(rgb(palette::FAINTER))
                        .child("/"),
                )
            })
    })
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

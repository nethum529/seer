use super::*;
use gpui::StyledImage;

impl Render for SeerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let reclaim = self.appearance.settings.hide_lines && self.appearance.settings.hide_top_bar;
        cx.set_global(crate::appearance::Glass {
            enabled: self.appearance.settings.glass,
            darkness: f32::from(self.appearance.settings.glass_darkness) / 100.,
            uniform: self.appearance.settings.uniform_glass,
            hide_lines: self.appearance.settings.hide_lines,
            reclaim_top: reclaim,
        });
        let menus = self.menu(window, cx);
        let people = self.people_menu(window, cx);
        let rooms = self.room_overlays(window, cx);
        let settings = self.settings_dialog(window, cx);
        let top_bar_opacity = self.top_bar_opacity(window, cx);
        let sidebar_width = self.sidebar_width(window, cx);
        let slide = px(if reclaim { 44. * top_bar_opacity } else { 0. });
        let controls = self.top_bar_controls(window, top_bar_opacity);
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
            .overflow_hidden()
            .flex()
            .bg(rgb(palette::WINDOW))
            .font_family(self.ui_font.clone())
            .text_size(px(13.))
            .line_height(relative(18. / 13.))
            .text_color(rgb(palette::TEXT))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &gpui::MouseDownEvent, _window, cx| {
                    view.keys.mode = Mode::Pointer;
                    if event.position.y < px(44.) {
                        #[cfg(target_os = "macos")]
                        if let Some(frame) = &mut view.input.frame {
                            frame.start_drag();
                        }
                        // GPUI's headless platform does not implement native window moves.
                        #[cfg(all(target_os = "linux", not(test)))]
                        _window.start_window_move();
                    }
                    cx.notify();
                }),
            )
            .on_action(cx.listener(Self::show_invite))
            .capture_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                view.travel(&event.keystroke, window, cx)
            }))
            .when(self.appearance.settings.glass, |root| {
                root.bg(gpui::black())
                    .children(self.appearance.picture.as_ref().map(|picture| {
                        div()
                            .absolute()
                            .inset_0()
                            .size_full()
                            .bg(gpui::black())
                            .children(
                                picture
                                    .blend(
                                        self.appearance.settings.glass_blur,
                                        self.appearance.settings.picture_brightness,
                                    )
                                    .into_iter()
                                    .map(|(layer, opacity)| {
                                        gpui::img(layer)
                                            .object_fit(gpui::ObjectFit::Cover)
                                            .absolute()
                                            .inset_0()
                                            .size_full()
                                            .opacity(opacity)
                                    }),
                            )
                    }))
            })
            .child(top_bar::events(cx));
        if let Some(join) = &self.join_screen {
            return root
                .child(join.clone())
                .child(self.no_room_controls(window, cx))
                .children(rooms)
                .children(self.invite.as_ref().map(|sheet| sheet.view.clone()))
                .children(settings)
                .children(controls);
        }
        let (link, name) = match &self.content {
            Content::Start(start) => {
                return root
                    .child(self.start_screen(start, window, cx))
                    .child(self.no_room_controls(window, cx))
                    .children(rooms)
                    .children(settings)
                    .children(controls);
            }
            Content::Message(message) => {
                return root
                    .child(self.message_content(message, window, cx))
                    .children(rooms)
                    .children(self.invite.as_ref().map(|sheet| sheet.view.clone()))
                    .children(settings)
                    .children(controls);
            }
            Content::Terminal(link, name) => (link, name),
        };
        let name: SharedString = chrome::capitalize(
            link.people
                .person()
                .map_or(name.as_ref(), |person| &person.name),
        )
        .into();
        let readonly = link.remote();
        let can_input = link.can_input();
        let person_open = self
            .people_menu
            .as_ref()
            .is_some_and(|menu| !menu.motion.closing());
        let labels = rename::Labels::new(
            self.editing.as_ref(),
            self.menu.as_ref(),
            readonly,
            !self.room_blocks_terminal_pointer() && self.menu.is_none(),
        );
        let terminals = link.terminals();
        let shown = link.shown();
        let terminal = if readonly && shown.is_none() {
            empty_terminal(
                if link.people.person().is_some_and(|person| person.peekable) {
                    "No terminals"
                } else {
                    "Offline"
                },
            )
            .into_any_element()
        } else {
            self.terminal(Screen::new(cx.entity(), self.font.clone()), cx)
                .relative()
                .when(shown.is_none(), |terminal| {
                    terminal.child(empty_terminal("No terminals").absolute().inset_0())
                })
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
                        .relative()
                        .child(
                            chrome::bar(cx)
                                .when(reclaim, |bar| bar.opacity(top_bar_opacity))
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
                                        .child(sidebar::new_terminal(
                                            &self.keys,
                                            true,
                                            labels.tips,
                                            window,
                                            cx,
                                        ))
                                        .when(readonly, |bar| bar.child(people::control(can_input)))
                                        .child(self.settings_button(window, cx)),
                                ),
                        )
                        .child(slide::Slide::new(
                            div()
                                .flex_1()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .when(reclaim, |body| {
                                    body.flex_none()
                                        .flex_basis(gpui::Length::Auto)
                                        .h(window.viewport_size().height)
                                })
                                .when(!terminals.is_empty(), |main| {
                                    main.child(sidebar::strip(
                                        terminals, shown, labels, &self.keys, window, cx,
                                    ))
                                })
                                .children(self.status_banner(window, cx))
                                .child(terminal),
                            if reclaim { slide - px(44.) } else { px(0.) },
                        )),
                )
                .children(menus)
                .children(people)
                .children(rooms)
                .children(self.invite.as_ref().map(|sheet| sheet.view.clone()))
                .children(settings)
                .children(controls);
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
            chrome::footer(
                self.room_button(link.room(), false, window, cx),
                self.settings_button(window, cx),
                cx,
            ),
        );
        let side = div().flex_none().w(sidebar_width).h_full().relative().when(
            sidebar_width > px(0.),
            |container| {
                container.child(
                    side.absolute()
                        .left(sidebar_width - px(sidebar::WIDTH))
                        .top_0(),
                )
            },
        );
        let title = heading(
            title.as_ref(),
            labels,
            readonly.then_some(&name),
            &self.ui_font,
            self.keys.hover(),
            cx,
        );
        let heading = chrome::bar(cx)
            .when(reclaim, |bar| bar.opacity(top_bar_opacity))
            .relative()
            .justify_center()
            .px(px(14.))
            .child(
                title
                    .opacity(if reclaim { 1. } else { top_bar_opacity })
                    .max_w(
                        (window.viewport_size().width
                            - sidebar_width
                            - px(if readonly { 180. } else { 28. }))
                        .max(px(0.)),
                    ),
            )
            .when(readonly, |bar| {
                bar.child(
                    people::control(can_input)
                        .opacity(if reclaim { 1. } else { top_bar_opacity })
                        .absolute()
                        .right(px(14.))
                        .top(px(13.)),
                )
            });
        let (heading, overlay) = if reclaim {
            (
                None,
                Some(
                    heading
                        .absolute()
                        .top(px(0.))
                        .left(sidebar_width)
                        .right(px(0.)),
                ),
            )
        } else {
            (Some(heading), None)
        };
        root.child(slide::Slide::new(side, slide))
            .child(slide::Slide::new(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .h_full()
                    .children(heading)
                    .children(self.status_banner(window, cx))
                    .child(terminal),
                slide,
            ))
            .children(reclaim.then(|| {
                chrome::bar(cx)
                    .absolute()
                    .top(px(0.))
                    .left(px(0.))
                    .w(sidebar_width)
                    .opacity(top_bar_opacity)
            }))
            .children(overlay)
            .children(menus)
            .children(people)
            .children(rooms)
            .children(self.invite.as_ref().map(|sheet| sheet.view.clone()))
            .children(settings)
            .children(controls)
    }
}

impl SeerWindow {
    fn message_content(
        &self,
        message: &SharedString,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        if self.room.server.is_some() {
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
                .child(crate::label::text(message.clone()))
                .child(self.no_room_controls(window, cx))
        }
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
        tip::terminal_help(&title, cx),
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
                    .child(crate::label::text(name.clone())),
            )
            .when(terminal.is_some(), |title| {
                title.child(
                    div()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(rgb(palette::FAINTER))
                        .child(crate::label::text("/")),
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

fn empty_terminal(text: &'static str) -> Div {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(palette::FAINT))
        .child(crate::label::text(text))
}

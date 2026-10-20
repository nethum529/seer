use super::*;
use crate::menu;
use gpui::AnyElement;

impl SeerWindow {
    pub(in crate::window::settings) fn process_rows(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let state = &self.appearance.processes;
        let toolbar = div()
            .id("processes-toolbar")
            .flex_none()
            .when(!state.busy, |row| {
                row.debug_selector(|| "processes-idle".into())
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .flex_wrap()
                    .child(
                        menu::button(
                            "processes-refresh",
                            "Refresh",
                            &state.refresh,
                            None,
                            self.keys.mode,
                            window,
                        )
                        .when(state.busy, |button| button.opacity(0.6))
                        .on_click(cx.listener(|view, _, _, cx| view.load_processes(None, cx))),
                    )
                    .when(state.can_clean(), |toolbar| {
                        toolbar.child(
                            menu::button(
                                "processes-clean",
                                "Clean up",
                                &state.clean,
                                None,
                                self.keys.mode,
                                window,
                            )
                            .when(state.busy, |button| button.opacity(0.6))
                            .on_click(cx.listener(
                                |view, _, window, cx| {
                                    window.focus(&view.appearance.processes.refresh);
                                    view.load_processes(Some(ProcessAction::Clean), cx);
                                },
                            )),
                        )
                    })
                    .when(state.busy, |toolbar| {
                        toolbar.child(
                            div()
                                .text_color(rgb(palette::MUTED))
                                .child(crate::label::text("Reading...")),
                        )
                    }),
            )
            .children(
                state
                    .error
                    .as_ref()
                    .filter(|_| state.sheet.is_none())
                    .map(|error| {
                        div()
                            .id("processes-error")
                            .debug_selector(|| "processes-error".into())
                            .mt(px(12.))
                            .text_color(rgb(palette::DANGER))
                            .child(crate::label::tooltip(error.clone().into()))
                    }),
            );
        let mut rows = vec![toolbar.into_any_element()];
        rows.extend(
            state
                .rows
                .iter()
                .map(|row| self.process_row(row, window, cx).into_any_element()),
        );
        if state.rows.is_empty() && !state.busy && state.error.is_none() {
            rows.push(
                div()
                    .child(crate::label::text("No Seer processes on this computer."))
                    .into_any_element(),
            );
        }
        rows
    }

    fn process_row(&self, row: &Row, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let process = &row.process;
        let pid = process.pid();
        let kind = match process.kind() {
            Kind::Broker => "Room server",
            Kind::Runtime => "Runtime",
            Kind::Window => "Window",
        };
        div()
            .id(("process-row", pid))
            .debug_selector(move || format!("process-row-{pid}"))
            .flex_none()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .min_h(px(32.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .flex_wrap()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(crate::label::text(kind)),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(palette::FAINT))
                            .child(crate::label::text(pid.to_string())),
                    )
                    .child(div().flex_1())
                    .when(row.used_by_this_window, |header| {
                        header.child(
                            div()
                                .id("process-used")
                                .debug_selector(|| "process-used".into())
                                .text_size(px(12.))
                                .text_color(rgb(palette::MUTED))
                                .child(crate::label::text("Used by this window")),
                        )
                    })
                    .when(row.can_stop(), |header| {
                        header.child(
                            menu::button(
                                "process-stop",
                                "Stop",
                                &row.focus,
                                None,
                                self.keys.mode,
                                window,
                            )
                            .id(("process-stop", pid))
                            .debug_selector(move || format!("process-stop-{pid}"))
                            .when(self.appearance.processes.busy, |button| button.opacity(0.6))
                            .on_click(cx.listener(
                                move |view, event, window, cx| {
                                    if matches!(event, gpui::ClickEvent::Mouse(_)) {
                                        view.keys.mode = Mode::Pointer;
                                        view.confirm_process(pid, window, cx);
                                    }
                                },
                            )),
                        )
                    }),
            )
            .child(
                div()
                    .text_color(rgb(palette::TEXT_2))
                    .child(crate::label::tooltip(process.description().into())),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .text_color(rgb(palette::FAINT))
                    .font(gpui::Font {
                        features: gpui::FontFeatures(Arc::new(vec![("tnum".into(), 1)])),
                        ..gpui::font(self.ui_font.clone())
                    })
                    .child(crate::label::text(format!(
                        "Up {}   CPU {}",
                        process.uptime(),
                        process.cpu()
                    ))),
            )
    }

    pub(in crate::window::settings) fn process_sheet(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = &mut self.appearance.processes;
        if state.focus_refresh {
            state.focus_refresh = false;
            if self.appearance.section == Section::Processes {
                window.focus(&state.refresh);
            }
        }
        if state
            .sheet
            .as_ref()
            .is_some_and(|sheet| sheet.motion.gone())
        {
            state.sheet = None;
        }
        let sheet = state.sheet.as_ref()?;
        if sheet.motion.animating() {
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }
        let opacity = sheet.motion.value();
        let cancel = menu::button(
            "process-cancel",
            "Cancel",
            &sheet.cancel,
            None,
            self.keys.mode,
            window,
        )
        .on_click(cx.listener(|view, event, window, cx| {
            if matches!(event, gpui::ClickEvent::Mouse(_)) {
                view.keys.mode = Mode::Pointer;
                view.close_process_sheet(window, cx);
            }
        }));
        let confirm = menu::button(
            "process-confirm",
            if state.busy {
                "Working..."
            } else {
                "Stop runtime"
            },
            &sheet.confirm,
            Some(palette::DANGER),
            self.keys.mode,
            window,
        )
        .when(state.busy, |button| button.opacity(0.6))
        .on_click(cx.listener(|view, event, _, cx| {
            if matches!(event, gpui::ClickEvent::Mouse(_)) {
                view.stop_process(cx);
            }
        }));
        let card = menu::confirm::card(
            menu::confirm::Copy {
                id: "process-confirm-sheet",
                title: "Stop this runtime?",
                body: "Its shells will end. Its windows lose their terminals.",
                error: state.error.as_deref(),
            },
            cancel,
            confirm,
            window,
            cx,
        );
        Some(
            menu::scrim("process-confirm-scrim")
                .opacity(opacity)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, window, cx| {
                        view.close_process_sheet(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(card)
                .into_any_element(),
        )
    }
}

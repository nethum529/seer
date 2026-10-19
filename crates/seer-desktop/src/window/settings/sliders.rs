use super::*;
use crate::menu::button;
use gpui::{FocusHandle, MouseMoveEvent, MouseUpEvent, Pixels, Point, canvas};

pub(in crate::window) const IDS: [&str; 3] = ["picture-brightness", "glass-darkness", "glass-blur"];
const LABELS: [&str; 3] = ["Picture brightness", "Glass darkness", "Glass blur"];
const RESET_IDS: [&str; 3] = ["reset-brightness", "reset-darkness", "reset-blur"];
const MAX: [u8; 3] = [100, 100, 40];

pub(in crate::window) fn value(settings: &seer::Settings, index: usize) -> u8 {
    [
        settings.picture_brightness,
        settings.glass_darkness,
        settings.glass_blur,
    ][index]
}

impl SeerWindow {
    pub(in crate::window) fn set_adjustment(
        &mut self,
        index: usize,
        value: u8,
        cx: &mut Context<Self>,
    ) {
        let value = value.min(MAX[index]);
        self.appearance.settings.glass = true;
        match index {
            0 => {
                self.appearance.settings.picture_brightness = value;
            }
            1 => self.appearance.settings.glass_darkness = value,
            _ => self.appearance.settings.glass_blur = value,
        }
        cx.notify();
    }

    fn save_adjustments(&mut self, cx: &mut Context<Self>) {
        self.appearance.save(self.appearance.settings.clone());
        cx.notify();
    }

    pub(super) fn adjustment_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.appearance.section != Section::Appearance {
            return false;
        }
        if let Some(index) = self
            .appearance
            .sliders
            .iter()
            .position(|focus| focus.is_focused(window))
        {
            let current = value(&self.appearance.settings, index);
            let next = match key.key.as_str() {
                "left" | "down" => current.saturating_sub(1),
                "right" | "up" => current.saturating_add(1),
                "home" => 0,
                "end" => MAX[index],
                _ => return false,
            };
            self.set_adjustment(index, next, cx);
            self.save_adjustments(cx);
            return true;
        }
        if matches!(key.key.as_str(), "enter" | "space")
            && let Some(index) = self
                .appearance
                .slider_reset
                .iter()
                .position(|focus| focus.is_focused(window))
        {
            self.set_adjustment(index, value(&seer::Settings::default(), index), cx);
            self.save_adjustments(cx);
            return true;
        }
        false
    }

    fn adjustment_pointer(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.appearance.slider_bounds[index].get();
        let fraction = f32::from(position.x - bounds.left() - px(8.))
            / f32::from(bounds.size.width - px(16.)).max(1.);
        self.set_adjustment(
            index,
            (fraction.clamp(0., 1.) * f32::from(MAX[index])).round() as u8,
            cx,
        );
    }

    pub(super) fn adjustment_row(
        &self,
        index: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let narrow = window.viewport_size().width < px(600.);
        let current = value(&self.appearance.settings, index);
        let focus = &self.appearance.sliders[index];
        let bounds = self.appearance.slider_bounds[index].clone();
        let ring = self.keys.ring(focus, window);
        let fraction = f32::from(current) / f32::from(MAX[index]);
        let text = format!("{current}{}", if index == 2 { " px" } else { "%" });
        let label = div()
            .flex_1()
            .min_w_0()
            .when(narrow, |label| {
                label
                    .flex_none()
                    .flex_basis(px(24.))
                    .w_full()
                    .h(px(24.))
                    .flex()
                    .items_center()
            })
            .font_weight(FontWeight::MEDIUM)
            .child(crate::label::text(LABELS[index]));
        let controls = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .when(narrow, |controls| controls.w_full().h(px(32.)))
            .child(
                div()
                    .w(px(48.))
                    .when(narrow, |value| value.flex_1())
                    .text_right()
                    .text_color(rgb(palette::MUTED))
                    .font(gpui::Font {
                        features: gpui::FontFeatures(Arc::new(vec![("tnum".into(), 1)])),
                        ..gpui::font(self.ui_font.clone())
                    })
                    .child(crate::label::text(text)),
            )
            .child(
                button(
                    RESET_IDS[index],
                    "Reset",
                    &self.appearance.slider_reset[index],
                    None,
                    self.keys.mode,
                    window,
                )
                .px(px(8.))
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.set_adjustment(index, value(&seer::Settings::default(), index), cx);
                    view.save_adjustments(cx);
                })),
            );
        div()
            .flex_none()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(if narrow { 56. } else { 32. }))
                    .flex()
                    .items_center()
                    .when(narrow, |header| header.flex_col().items_start())
                    .gap(px(if narrow { 0. } else { 8. }))
                    .child(label)
                    .child(controls),
            )
            .child(
                div()
                    .id(IDS[index])
                    .debug_selector(move || IDS[index].into())
                    .track_focus(focus)
                    .relative()
                    .h(px(24.))
                    .w_full()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(ring, |track| track.child(sidebar::outline()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            view.keys.mode = Mode::Pointer;
                            window.focus(&view.appearance.sliders[index]);
                            view.appearance.slider_drag = Some(index);
                            view.adjustment_pointer(index, event.position, cx);
                        }),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(8.))
                            .right(px(8.))
                            .top(px(10.))
                            .h(px(4.))
                            .rounded(px(2.))
                            .bg(rgb(palette::FAINTER))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(fraction))
                                    .rounded(px(2.))
                                    .bg(rgb(palette::TEXT)),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(8.))
                            .right(px(8.))
                            .top(px(4.))
                            .h(px(16.))
                            .child(
                                div()
                                    .relative()
                                    .left(relative(fraction))
                                    .ml(px(-8.))
                                    .size(px(16.))
                                    .rounded(px(8.))
                                    .bg(rgb(palette::TEXT)),
                            ),
                    )
                    .child(
                        canvas(move |rect, _, _| bounds.set(rect), |_, _, _, _| {})
                            .absolute()
                            .inset_0(),
                    ),
            )
    }

    pub(super) fn adjustment_events(&self, cx: &Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let moving = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase.capture() {
                        let _ = moving.update(cx, |view, cx| {
                            if let Some(index) = view.appearance.slider_drag {
                                if event.pressed_button == Some(MouseButton::Left) {
                                    view.adjustment_pointer(index, event.position, cx);
                                } else {
                                    view.appearance.slider_drag = None;
                                    view.save_adjustments(cx);
                                }
                                cx.stop_propagation();
                            }
                        });
                    }
                });
                let released = view.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase.capture() && event.button == MouseButton::Left {
                        let _ = released.update(cx, |view, cx| {
                            if view.appearance.slider_drag.take().is_some() {
                                view.save_adjustments(cx);
                                cx.stop_propagation();
                            }
                        });
                    }
                });
            },
        )
        .absolute()
        .inset_0()
    }

    pub(super) fn adjustment_focus(&self) -> impl Iterator<Item = &FocusHandle> {
        self.appearance
            .sliders
            .iter()
            .zip(&self.appearance.slider_reset)
            .flat_map(|(slider, reset)| [slider, reset])
    }

    pub(super) fn reveal_adjustment(&self, focus: &FocusHandle) -> bool {
        for index in 0..3 {
            if focus == &self.appearance.sliders[index]
                || focus == &self.appearance.slider_reset[index]
            {
                self.appearance.scroll.scroll_to_item(index);
                return true;
            }
        }
        false
    }
}

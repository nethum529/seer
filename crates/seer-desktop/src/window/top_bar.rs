use super::*;
use crate::motion::Motion;

pub(crate) struct TopBar {
    motion: Motion,
    pub(super) hovered: bool,
}

impl TopBar {
    pub(crate) fn new() -> Self {
        let mut motion = Motion::open(true, 0);
        motion.close(true, 0);
        Self {
            motion,
            hovered: false,
        }
    }
}

impl SeerWindow {
    pub(super) fn top_bar_opacity(&mut self, window: &mut Window, cx: &mut Context<Self>) -> f32 {
        let focused = window.viewport_size().width < px(NARROW)
            && self.keys.mode == Mode::Keyboard
            && [
                &self.keys.who,
                &self.keys.new_terminal,
                &self.room.focus,
                &self.appearance.trigger,
            ]
            .iter()
            .any(|focus| focus.is_focused(window));
        let pinned = !self.appearance.settings.hide_top_bar
            || !matches!(self.content, Content::Terminal(..))
            || self.appearance.motion.is_some()
            || self.menu.is_some()
            || self.people_menu.is_some()
            || self.room.open()
            || self.room.modal()
            || self.invite.is_some()
            || self.editing.is_some()
            || focused;
        let bar = &mut self.appearance.top_bar;
        let shown = pinned || bar.hovered;
        if shown == bar.motion.closing() {
            if shown {
                bar.motion.reopen(pinned, 120);
            } else {
                bar.motion.close(false, 80);
            }
        }
        if bar.motion.animating() {
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }
        let opacity = bar.motion.value();
        #[cfg(target_os = "macos")]
        if let Some(frame) = &mut self.input.frame {
            frame.top_bar_opacity(opacity);
        }
        opacity
    }

    pub(super) fn top_bar_controls(&self, _window: &Window, _opacity: f32) -> Option<Div> {
        #[cfg(target_os = "linux")]
        {
            let narrow = _window.viewport_size().width < px(NARROW)
                && !matches!(self.content, Content::Join(_));
            Some(
                chrome::window_controls(_opacity)
                    .absolute()
                    .top(px(0.))
                    .left(px(if narrow { 14. } else { 20. })),
            )
        }
        #[cfg(target_os = "macos")]
        None
    }

    pub(super) fn hover_top_bar(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.appearance.top_bar.hovered != hovered {
            self.appearance.top_bar.hovered = hovered;
            cx.notify();
        }
    }
}

pub(super) fn events(cx: &Context<SeerWindow>) -> impl IntoElement {
    let view = cx.entity().downgrade();
    gpui::canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            let top = gpui::Bounds {
                origin: gpui::point(px(0.), px(0.)),
                size: gpui::size(window.viewport_size().width, px(44.)),
            };
            let view = view.clone();
            let exited = view.clone();
            #[cfg(target_os = "macos")]
            let released = view.clone();
            window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                if phase.capture() {
                    let _ = view.update(cx, |view, cx| {
                        view.hover_top_bar(top.contains(&event.position), cx);
                        #[cfg(target_os = "macos")]
                        if event.pressed_button == Some(MouseButton::Left)
                            && let Some(frame) = &view.input.frame
                            && frame.drag(cx)
                        {
                            view.hover_top_bar(true, cx);
                        }
                    });
                }
            });
            #[cfg(target_os = "macos")]
            window.on_mouse_event(move |_: &gpui::MouseUpEvent, phase, _, cx| {
                if phase.capture() {
                    let _ = released.update(cx, |view, cx| {
                        if let Some(frame) = &mut view.input.frame
                            && frame.finish_drag()
                        {
                            view.hover_top_bar(true, cx);
                        }
                    });
                }
            });
            window.on_mouse_event(move |_: &gpui::MouseExitEvent, phase, _, cx| {
                if phase.capture() {
                    let _ = exited.update(cx, |view, cx| view.hover_top_bar(false, cx));
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

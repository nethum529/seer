use super::reveal::Reveal;
use super::*;

pub(super) fn centered_title(title: impl IntoElement) -> impl IntoElement {
    let title = title.into_any_element();
    gpui::canvas(
        move |bounds, window, cx| {
            let width = window.viewport_size().width;
            let inset = bounds.left().max(width - bounds.right());
            let available = (width - inset * 2.).max(px(0.));
            let mut title = div()
                .w(available)
                .h(bounds.size.height)
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .child(title)
                .into_any_element();
            title.prepaint_as_root(
                gpui::point(inset, bounds.top()),
                gpui::size(available.into(), bounds.size.height.into()),
                window,
                cx,
            );
            title
        },
        |_, mut title, window, cx| title.paint(window, cx),
    )
    .flex_1()
    .min_w_0()
    .h_full()
}

pub(crate) struct TopBar {
    motion: Reveal,
    pub(super) hovered: bool,
}

impl TopBar {
    pub(crate) fn new() -> Self {
        Self {
            motion: Reveal::new(),
            hovered: false,
        }
    }
}

impl SeerWindow {
    pub(super) fn top_bar_opacity(&mut self, window: &mut Window, cx: &mut Context<Self>) -> f32 {
        let narrow = window.viewport_size().width < px(NARROW);
        let focused = narrow
            && self.keys.mode == Mode::Keyboard
            && [
                &self.keys.who,
                &self.keys.new_terminal,
                &self.room.focus,
                &self.appearance.trigger,
            ]
            .iter()
            .any(|focus| focus.is_focused(window));
        let top_bar_popup = narrow
            && (self.appearance.motion.is_some() || self.people_menu.is_some() || self.room.open());
        let pinned = !self.appearance.settings.hide_top_bar
            || !matches!(self.content, Content::Terminal(..))
            || self.join_screen.is_some()
            || top_bar_popup
            || focused;
        let bar = &mut self.appearance.top_bar;
        let shown = pinned || bar.hovered;
        if shown == bar.motion.closing() {
            let instant = pinned || self.keys.mode == Mode::Keyboard || reduced_motion();
            bar.motion.retarget(shown, instant);
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
                && self.join_screen.is_none()
                && !matches!(self.content, Content::Start(_));
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
            self.keys.mode = Mode::Pointer;
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
                        let mut area = top;
                        if view.appearance.settings.hide_lines
                            && view.appearance.settings.hide_top_bar
                        {
                            area.size.height += px(44. * view.appearance.top_bar.motion.value());
                        }
                        view.hover_top_bar(area.contains(&event.position), cx);
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

#[cfg(target_os = "macos")]
pub(super) fn reduced_motion() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

#[cfg(target_os = "linux")]
pub(super) fn reduced_motion() -> bool {
    use std::process::Command;
    let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| desktop.contains("KDE"));
    let output = if kde {
        Command::new("kreadconfig6")
            .args(["--group", "KDE", "--key", "AnimationDurationFactor"])
            .output()
    } else {
        Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "enable-animations"])
            .output()
    };
    output
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            let value = String::from_utf8_lossy(&output.stdout);
            if kde {
                value.trim().parse::<f32>() == Ok(0.)
            } else {
                value.trim() == "false"
            }
        })
}

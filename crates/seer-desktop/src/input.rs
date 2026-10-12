use gpui::{
    ClipboardItem, Context, Div, FocusHandle, InteractiveElement, KeyDownEvent, Keystroke,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point,
    ScrollWheelEvent, Stateful, Styled, Window, div, px,
};
use seer_core::{InputEvent, MouseInput, MouseKind, MouseTracking, TerminalInput};

use crate::composition::Composition;
use crate::keyboard;
use crate::link::{Shown, lock};
use crate::screen::{Grid, Screen};
use crate::sidebar::Mode;
use crate::window::{Content, SeerWindow};

pub(crate) struct Input {
    #[cfg(target_os = "macos")]
    pub(crate) _native_keys: Option<crate::native_keys::NativeKeys>,
    #[cfg(target_os = "macos")]
    pub(crate) frame: Option<crate::native_frame::NativeFrame>,
    #[cfg(target_os = "macos")]
    pub(crate) waiting_echo: bool,
    pub(crate) focus: FocusHandle,
    pub(crate) grid: Option<Grid>,
    pub(crate) composition: Composition,
    dragging: Option<MouseButton>,
    last_mouse: Option<MouseInput>,
    wheel: Point<f32>,
}

impl Input {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<SeerWindow>) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        cx.on_focus(&focus, window, |view, _, cx| {
            view.send_input(InputEvent::Focus(true), cx);
            cx.notify();
        })
        .detach();
        cx.on_blur(&focus, window, |view, _, cx| {
            view.input.composition = Composition::default();
            view.send_input(InputEvent::Focus(false), cx);
            cx.notify();
        })
        .detach();
        cx.observe_window_activation(window, |view, window, cx| {
            if view.input.focus.is_focused(window) {
                view.send_input(InputEvent::Focus(window.is_window_active()), cx);
            }
            cx.notify();
        })
        .detach();
        Self {
            #[cfg(target_os = "macos")]
            _native_keys: None,
            #[cfg(target_os = "macos")]
            frame: None,
            #[cfg(target_os = "macos")]
            waiting_echo: false,
            focus,
            grid: None,
            composition: Composition::default(),
            dragging: None,
            last_mouse: None,
            wheel: Point::default(),
        }
    }

    pub(crate) fn reset(&mut self) {
        #[cfg(target_os = "macos")]
        {
            self.waiting_echo = false;
        }
        self.composition = Composition::default();
        self.dragging = None;
        self.last_mouse = None;
        self.wheel = Point::default();
    }
}

impl SeerWindow {
    pub(crate) fn terminal(&self, screen: Screen, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut area = div()
            .id("terminal")
            .debug_selector(|| "terminal".into())
            .track_focus(&self.input.focus)
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .py(px(8.))
            .px(px(12.))
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
                view.terminal_key(&event.keystroke, cx)
            }))
            .on_any_mouse_down(
                cx.listener(|view, event, window, cx| view.mouse_down(event, window, cx)),
            )
            .on_scroll_wheel(cx.listener(|view, event, _, cx| view.wheel(event, cx)))
            .child(screen);
        area.interactivity()
            .on_any_mouse_up(cx.listener(|view, event, _, cx| view.mouse_up(event, cx)));
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            area = area.on_mouse_up_out(
                button,
                cx.listener(|view, event, _, cx| view.mouse_up(event, cx)),
            );
        }
        area
    }

    pub(crate) fn send_input(&mut self, event: InputEvent, cx: &mut Context<Self>) {
        if matches!(&self.content, Content::Terminal(link, _) if link.readonly()) {
            return;
        }
        let changed = if matches!(
            event,
            InputEvent::Text(_) | InputEvent::Key(_) | InputEvent::Paste(_)
        ) {
            #[cfg(target_os = "macos")]
            {
                self.input.waiting_echo = true;
            }
            self.with_raw(|grid| grid.input_started()).unwrap_or(false)
        } else {
            false
        };
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.input(event),
            Content::Message(_) | Content::Join(_) => Ok(()),
        };
        self.check(result, cx);
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn with_raw<T>(
        &self,
        f: impl FnOnce(&mut seer_runtime::PaneGrid) -> T,
    ) -> Option<T> {
        let Content::Terminal(link, _) = &self.content else {
            return None;
        };
        let mut held = lock(link.terminal());
        let Some(Shown::Raw(grid)) = held.as_mut() else {
            return None;
        };
        Some(f(grid))
    }

    pub(crate) fn focus_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.invite_open() || !matches!(self.content, Content::Terminal(_, _)) {
            return;
        }
        self.keys.mode = Mode::Keyboard;
        window.focus(&self.keys.tabs);
        cx.notify();
    }

    pub(crate) fn terminal_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) {
        if self.input.composition.marked() {
            return;
        }
        if self.scroll_key(key) {
            cx.notify();
            cx.stop_propagation();
        } else if clipboard_shortcut(key) {
            match key.key.as_str() {
                "c" => self.copy(cx),
                "v" => self.paste(cx),
                _ => {}
            }
            cx.stop_propagation();
        } else if let Some(event) = keyboard::event(key) {
            self.send_input(event, cx);
            cx.stop_propagation();
        } else if key.modifiers.platform {
            cx.stop_propagation();
        }
    }

    fn scroll_key(&self, key: &Keystroke) -> bool {
        let held = key.modifiers;
        if !held.shift || held.control || held.alt || held.platform {
            return false;
        }
        let rows = self.input.grid.map_or(1, |grid| grid.rows as i32);
        let lines = match key.key.as_str() {
            "home" => i32::MAX,
            "end" => -i32::MAX,
            "pageup" => rows,
            "pagedown" => -rows,
            _ => return false,
        };
        self.with_raw(|grid| {
            if grid.modes().alt_screen {
                return false;
            }
            grid.handle_input(&TerminalInput::new(InputEvent::Scrollback { lines }))
                .ok();
            true
        })
        .unwrap_or(false)
    }

    fn copy(&self, cx: &mut Context<Self>) {
        if let Some(Some(text)) = self.with_raw(|grid| grid.selection_text()) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.send_input(InputEvent::Paste(text), cx);
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.room_blocks_terminal_pointer() {
            cx.stop_propagation();
            return;
        }
        window.focus(&self.input.focus);
        self.keys.mode = Mode::Pointer;
        self.input.dragging = Some(event.button);
        if self.reporting(event.modifiers.shift) {
            self.mouse_report(
                event.position,
                MouseKind::Down,
                Some(event.button),
                event.modifiers,
                cx,
            );
        } else if event.button == MouseButton::Left
            && let Some((column, row, right)) = self.hit(event.position)
        {
            self.with_raw(|grid| grid.start_selection(column, row, right, event.click_count));
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.room_blocks_terminal_pointer() {
            return;
        }
        if self.input.dragging.is_none()
            && !self
                .input
                .grid
                .is_some_and(|grid| grid.contains(event.position))
        {
            return;
        }
        if self.reporting(event.modifiers.shift) {
            let kind = if self.input.dragging.is_some() {
                MouseKind::Drag
            } else {
                MouseKind::Moved
            };
            self.mouse_report(
                event.position,
                kind,
                self.input.dragging.or(event.pressed_button),
                event.modifiers,
                cx,
            );
        } else if self.input.dragging == Some(MouseButton::Left)
            && let Some((column, row, right)) = self.hit(event.position)
        {
            self.with_raw(|grid| grid.update_selection(column, row, right));
            cx.notify();
        }
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        if self.room_blocks_terminal_pointer() {
            return;
        }
        let dragging = self.input.dragging.take();
        if dragging.is_some() && self.reporting(event.modifiers.shift) {
            self.mouse_report(
                event.position,
                MouseKind::Up,
                Some(event.button),
                event.modifiers,
                cx,
            );
        } else if dragging == Some(MouseButton::Left)
            && let Some((column, row, right)) = self.hit(event.position)
        {
            self.with_raw(|grid| grid.update_selection(column, row, right));
        }
        cx.notify();
    }

    fn wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if self.room_blocks_terminal_pointer() {
            cx.stop_propagation();
            return;
        }
        let Some(grid) = self.input.grid else {
            return;
        };
        let mut delta = event.delta.pixel_delta(grid.line_height);
        if !event.delta.precise() {
            delta *= 3.;
        }
        self.input.wheel.y += delta.y / grid.line_height;
        self.input.wheel.x += delta.x / grid.cell_width;
        let y = self.input.wheel.y.trunc() as i32;
        let x = self.input.wheel.x.trunc() as i32;
        self.input.wheel.y -= y as f32;
        self.input.wheel.x -= x as f32;
        if self.reporting(event.modifiers.shift) {
            self.wheel_reports(event, y, MouseKind::ScrollUp, MouseKind::ScrollDown, cx);
            self.wheel_reports(event, x, MouseKind::ScrollLeft, MouseKind::ScrollRight, cx);
        } else if self
            .with_raw(|grid| grid.alternate_scroll())
            .unwrap_or(false)
            && !event.modifiers.shift
        {
            let code = if y > 0 {
                seer_core::KeyCode::Up
            } else {
                seer_core::KeyCode::Down
            };
            for _ in 0..y.unsigned_abs() {
                self.send_input(
                    InputEvent::Key(seer_core::KeyInput {
                        code,
                        modifiers: seer_core::Modifiers::default(),
                    }),
                    cx,
                );
            }
        } else {
            self.with_raw(|grid| {
                grid.handle_input(&TerminalInput::new(InputEvent::Scrollback { lines: y }))
                    .ok()
            });
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn wheel_reports(
        &mut self,
        event: &ScrollWheelEvent,
        lines: i32,
        up: MouseKind,
        down: MouseKind,
        cx: &mut Context<Self>,
    ) {
        let kind = if lines > 0 { up } else { down };
        for _ in 0..lines.unsigned_abs() {
            self.mouse_report(event.position, kind, None, event.modifiers, cx);
        }
    }

    fn reporting(&self, shift: bool) -> bool {
        if shift {
            return false;
        }
        match &self.content {
            Content::Terminal(link, _) => {
                lock(link.terminal())
                    .as_ref()
                    .is_some_and(|shown| match shown {
                        Shown::Raw(grid) => grid.modes().mouse_tracking != MouseTracking::None,
                        Shown::Frame { frame, .. } => {
                            frame.modes.mouse_tracking != MouseTracking::None
                        }
                    })
            }
            Content::Message(_) | Content::Join(_) => false,
        }
    }

    fn hit(&self, position: Point<Pixels>) -> Option<(u16, u16, bool)> {
        self.input.grid?.hit(position)
    }

    fn mouse_report(
        &mut self,
        position: Point<Pixels>,
        kind: MouseKind,
        button: Option<MouseButton>,
        held: gpui::Modifiers,
        cx: &mut Context<Self>,
    ) {
        let Some((column, row, _)) = self.hit(position) else {
            return;
        };
        let mouse = MouseInput {
            kind,
            button: button.and_then(mouse_button),
            column,
            row,
            modifiers: keyboard::modifiers(held),
        };
        if matches!(kind, MouseKind::Moved | MouseKind::Drag)
            && self.input.last_mouse == Some(mouse)
        {
            return;
        }
        self.input.last_mouse = Some(mouse);
        self.send_input(InputEvent::Mouse(mouse), cx);
    }
}

fn clipboard_shortcut(key: &Keystroke) -> bool {
    let held = key.modifiers;
    let copy = matches!(key.key.as_str(), "c" | "v");
    let modifier = if cfg!(target_os = "macos") {
        held.platform && !held.control
    } else {
        held.control && held.shift && !held.platform
    };
    copy && modifier && !held.alt
}

fn mouse_button(button: MouseButton) -> Option<seer_core::MouseButton> {
    match button {
        MouseButton::Left => Some(seer_core::MouseButton::Left),
        MouseButton::Middle => Some(seer_core::MouseButton::Middle),
        MouseButton::Right => Some(seer_core::MouseButton::Right),
        _ => None,
    }
}

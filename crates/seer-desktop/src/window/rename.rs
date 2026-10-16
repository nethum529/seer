use gpui::{AnyElement, Entity, FocusHandle, Pixels, Point, Subscription, point};
use gpui::{AppContext, StatefulInteractiveElement};
use seer_core::proto::TerminalInfo;

use super::*;
use crate::name_input::{Finished, NameInput};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Surface {
    Tab,
    Strip,
    Title,
}

#[derive(Clone, Copy)]
pub(crate) struct Labels<'a> {
    pub(crate) readonly: bool,
    pub(crate) tips: bool,
    editing: Option<&'a Editing>,
    menu: Option<&'a Menu>,
}

impl<'a> Labels<'a> {
    pub(super) fn new(
        editing: Option<&'a Editing>,
        menu: Option<&'a Menu>,
        readonly: bool,
        tips: bool,
    ) -> Self {
        Self {
            editing,
            menu,
            readonly,
            tips,
        }
    }

    pub(crate) fn active(&self, terminal: &TerminalInfo) -> bool {
        self.editing
            .is_some_and(|editing| editing.pane == terminal.pane)
            || self.menu.is_some_and(|menu| menu.pane == terminal.pane)
    }
}

pub(crate) struct Editing {
    pane: String,
    surface: Surface,
    input: Entity<NameInput>,
    _events: Subscription,
    _blur: Subscription,
}

impl Editing {
    fn matches(&self, terminal: &TerminalInfo, surface: Surface) -> bool {
        self.pane == terminal.pane && self.surface == surface
    }
}

pub(super) struct Menu {
    pane: String,
    surface: Surface,
    position: Point<Pixels>,
    focus: FocusHandle,
    selected: usize,
    motion: crate::motion::Motion,
}

impl SeerWindow {
    pub(crate) fn rename(
        &mut self,
        pane: &str,
        surface: Surface,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_name(true, false, window, cx);
        let Content::Terminal(link, _) = &self.content else {
            return;
        };
        if link.remote() {
            return;
        }
        let Some(terminal) = link
            .terminals()
            .iter()
            .find(|terminal| terminal.pane == pane)
        else {
            return;
        };
        let Some(title) = terminal.title.clone() else {
            return;
        };
        let placeholder = chrome::capitalize(&terminal.name);
        let input = cx.new(|cx| NameInput::new(title, placeholder, cx));
        input.update(cx, |input, _| input.centered = surface == Surface::Title);
        let events = cx.subscribe_in(&input, window, |view, _, event, window, cx| {
            if matches!(event, Finished::Changed) {
                return;
            }
            view.finish_name(
                !matches!(event, Finished::Cancel),
                !matches!(event, Finished::SaveOutside),
                window,
                cx,
            );
        });
        let focus = input.read(cx).focus.clone();
        let blur = cx.on_blur(&focus, window, |view, window, cx| {
            view.finish_name(true, false, window, cx)
        });
        self.editing = Some(Editing {
            pane: pane.into(),
            surface,
            input,
            _events: events,
            _blur: blur,
        });
        self.menu = None;
        window.focus(&focus);
        cx.notify();
    }

    pub(super) fn finish_name(
        &mut self,
        save: bool,
        restore_focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editing) = self.editing.take() else {
            return;
        };
        if save {
            let title = editing.input.read(cx).text.clone();
            let result = match &mut self.content {
                Content::Terminal(link, _) => link.rename(&editing.pane, title),
                Content::Message(_) | Content::Join(_) => Ok(()),
            };
            self.check(result, cx);
        }
        if restore_focus {
            window.focus(&self.keys.tabs);
        }
        cx.notify();
    }

    pub(crate) fn name_menu(
        &mut self,
        pane: &str,
        surface: Surface,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_name(true, false, window, cx);
        let Content::Terminal(link, _) = &self.content else {
            return;
        };
        if link.remote() {
            return;
        }
        if !link
            .terminals()
            .iter()
            .any(|terminal| terminal.pane == pane && terminal.title.is_some())
        {
            return;
        }
        let focus = cx.focus_handle();
        window.focus(&focus);
        self.menu = Some(Menu {
            pane: pane.into(),
            surface,
            position,
            focus,
            selected: 0,
            motion: crate::motion::Motion::open(self.keys.mode == Mode::Keyboard, 150),
        });
        self.notify_room(cx);
    }

    pub(super) fn rename_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.keys.tabs.is_focused(window)
            || key.modifiers.control
            || key.modifiers.alt
            || key.modifiers.platform
        {
            return false;
        }
        let Content::Terminal(link, _) = &self.content else {
            return false;
        };
        let Some(pane) = link.shown().map(str::to_owned) else {
            return false;
        };
        let surface = if window.viewport_size().width < px(NARROW) {
            Surface::Strip
        } else {
            Surface::Tab
        };
        self.keys.mode = Mode::Keyboard;
        match key.key.as_str() {
            "f2" if !key.modifiers.shift => self.rename(&pane, surface, window, cx),
            "f10" if key.modifiers.shift => {
                let at = link
                    .terminals()
                    .iter()
                    .position(|terminal| terminal.pane == pane);
                let position = at
                    .and_then(|at| self.keys.scroll.bounds_for_item(at))
                    .map(|bounds| point(bounds.left(), bounds.bottom()) + self.keys.scroll.offset())
                    .unwrap_or(point(px(8.), px(92.)));
                self.name_menu(&pane, surface, position, window, cx);
            }
            _ => return false,
        }
        true
    }

    fn dismiss_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.menu {
            menu.motion.close(self.keys.mode == Mode::Keyboard, 80);
        }
        window.focus(&self.keys.tabs);
        self.notify_room(cx);
    }

    pub(super) fn menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.menu.as_ref().is_some_and(|menu| menu.motion.gone()) {
            self.menu = None;
        }
        let menu = self.menu.as_ref()?;
        if menu.motion.animating() {
            cx.on_next_frame(window, |view, _, cx| view.notify_room(cx));
        }
        let viewport = window.viewport_size();
        let x = menu
            .position
            .x
            .clamp(px(8.), (viewport.width - px(192.)).max(px(8.)));
        let y = menu
            .position
            .y
            .clamp(px(8.), (viewport.height - px(82.)).max(px(8.)));
        Some(
            crate::menu::panel("name-menu", 184., cx)
                .debug_selector(|| "name-menu".into())
                .absolute()
                .left(x)
                .top(y)
                .opacity(menu.motion.value())
                .track_focus(&menu.focus)
                .on_mouse_down_out(cx.listener(|view, _, window, cx| view.dismiss_menu(window, cx)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                    view.keys.mode = Mode::Keyboard;
                    match event.keystroke.key.as_str() {
                        "enter" | "space" => view.pick_tab_action(window, cx),
                        "escape" | "tab" => view.dismiss_menu(window, cx),
                        "up" | "down" | "home" | "end" => {
                            if let Some(menu) = &mut view.menu {
                                menu.selected =
                                    sidebar::step(&event.keystroke.key, 2, Some(menu.selected))
                                        .unwrap_or(0);
                                view.notify_room(cx);
                            }
                        }
                        _ => {}
                    }
                    cx.stop_propagation();
                }))
                .child(menu_item(menu, 0, cx))
                .child(menu_item(menu, 1, cx))
                .into_any_element(),
        )
    }

    fn pick_tab_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = &self.menu else {
            return;
        };
        if menu.motion.closing() {
            return;
        }
        let pane = menu.pane.clone();
        let surface = menu.surface;
        if menu.selected == 0 {
            self.rename(&pane, surface, window, cx);
        } else {
            self.close_terminal(&pane, window, cx);
        }
    }
}

fn menu_item(menu: &Menu, index: usize, cx: &mut Context<SeerWindow>) -> Stateful<Div> {
    let (id, label, icon, color) = if index == 0 {
        ("rename", "Rename", "pencil", palette::TEXT_2)
    } else {
        ("close-terminal", "Close", "close", palette::DANGER)
    };
    crate::menu::item(id, 32.)
        .debug_selector(move || id.into())
        .when(menu.selected == index, |row| row.bg(rgb(palette::CHIP)))
        .text_color(rgb(if index == 0 {
            palette::TEXT
        } else {
            palette::DANGER
        }))
        .active(|row| row.opacity(0.84))
        .on_hover(cx.listener(move |view, hovered, _, cx| {
            if *hovered && let Some(menu) = &mut view.menu {
                menu.selected = index;
                view.notify_room(cx);
            }
        }))
        .on_click(cx.listener(move |view, _, window, cx| {
            if let Some(menu) = &mut view.menu {
                menu.selected = index;
            }
            view.pick_tab_action(window, cx);
        }))
        .child(crate::cap_icon::cap_icon(chrome::icon(icon, color)))
        .child(crate::label::text(label))
}

pub(crate) fn label(terminal: &TerminalInfo, surface: Surface, labels: Labels<'_>) -> AnyElement {
    if let Some(editing) = labels.editing
        && editing.matches(terminal, surface)
    {
        let field = div().min_w_0().child(editing.input.clone());
        return match surface {
            Surface::Tab => field.w_full(),
            Surface::Strip => field.w(px(140.)),
            Surface::Title => field.w(px(220.)),
        }
        .into_any_element();
    }
    sidebar::one_line(div())
        .child(crate::label::text(display_name(terminal)))
        .into_any_element()
}

pub(crate) fn display_name(terminal: &TerminalInfo) -> String {
    match terminal.title.as_deref().filter(|title| !title.is_empty()) {
        Some(title) => title.to_owned(),
        None => chrome::capitalize(&terminal.name),
    }
}

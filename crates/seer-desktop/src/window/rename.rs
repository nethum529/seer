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
    editing: Option<&'a Editing>,
    menu: Option<&'a Menu>,
}

impl<'a> Labels<'a> {
    pub(super) fn new(
        editing: Option<&'a Editing>,
        menu: Option<&'a Menu>,
        readonly: bool,
    ) -> Self {
        Self {
            editing,
            menu,
            readonly,
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
        if link.readonly() {
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
        if link.readonly() {
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
        });
        cx.notify();
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
        self.menu = None;
        window.focus(&self.keys.tabs);
        cx.notify();
    }

    pub(super) fn menu(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.menu.as_ref()?;
        let pane = menu.pane.clone();
        let surface = menu.surface;
        let viewport = window.viewport_size();
        let x = menu
            .position
            .x
            .clamp(px(8.), (viewport.width - px(192.)).max(px(8.)));
        let y = menu
            .position
            .y
            .clamp(px(8.), (viewport.height - px(50.)).max(px(8.)));
        Some(
            crate::menu::panel("name-menu", 184.)
                .absolute()
                .left(x)
                .top(y)
                .track_focus(&menu.focus)
                .on_mouse_down_out(cx.listener(|view, _, window, cx| view.dismiss_menu(window, cx)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "enter" | "space" => view.rename(&pane, surface, window, cx),
                        "escape" | "tab" => view.dismiss_menu(window, cx),
                        _ => {}
                    }
                    cx.stop_propagation();
                }))
                .child(menu_item(menu, cx))
                .into_any_element(),
        )
    }
}

fn menu_item(menu: &Menu, cx: &mut Context<SeerWindow>) -> Stateful<Div> {
    let pane = menu.pane.clone();
    let surface = menu.surface;
    crate::menu::item("rename", 32.)
        .bg(rgb(palette::CHIP))
        .text_color(rgb(palette::TEXT))
        .on_click(cx.listener(move |view, _, window, cx| view.rename(&pane, surface, window, cx)))
        .child(crate::cap_icon::cap_icon(chrome::icon(
            "pencil",
            palette::TEXT_2,
        )))
        .child("Rename")
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
        .child(display_name(terminal))
        .into_any_element()
}

pub(crate) fn display_name(terminal: &TerminalInfo) -> String {
    match terminal.title.as_deref().filter(|title| !title.is_empty()) {
        Some(title) => title.to_owned(),
        None => chrome::capitalize(&terminal.name),
    }
}

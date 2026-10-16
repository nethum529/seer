use super::*;
use crate::motion::Motion;

pub(crate) struct Sidebar {
    motion: Motion,
    pub(crate) moving: bool,
    return_focus: Option<gpui::FocusHandle>,
}

impl Sidebar {
    pub(crate) fn new(hidden: bool) -> Self {
        let mut motion = Motion::open(true, 0);
        if hidden {
            motion.close(true, 0);
        }
        Self {
            motion,
            moving: false,
            return_focus: None,
        }
    }
}

impl SeerWindow {
    pub(crate) fn sidebar_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let held = key.modifiers;
        if key.key != "b"
            || held.function
            || self.editing.is_some()
            || self.menu.is_some()
            || self.room.modal()
            || !matches!(self.content, Content::Terminal(..))
        {
            return false;
        }
        let (toggle, focus) = if cfg!(target_os = "macos") {
            let command = held.platform && !held.control && !held.alt;
            (command && !held.shift, command && held.shift)
        } else {
            let command = held.control && held.shift && !held.platform;
            (command && !held.alt, command && held.alt)
        };
        if toggle {
            let hidden = !self.appearance.settings.hide_sidebar;
            self.set_sidebar_hidden(hidden, top_bar::reduced_motion(), window, cx);
        } else if focus {
            self.focus_tabs(window, cx);
        }
        toggle || focus
    }

    pub(crate) fn set_sidebar_hidden(
        &mut self,
        hidden: bool,
        instant: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if hidden == self.appearance.settings.hide_sidebar {
            return;
        }
        let mut settings = self.appearance.settings.clone();
        settings.hide_sidebar = hidden;
        if !self.appearance.save(settings) {
            return;
        }
        let return_focus = if self.people_menu.is_some() {
            Some(self.keys.who.clone())
        } else if self.room.open() {
            Some(self.room.focus.clone())
        } else {
            window.focused(cx)
        };
        self.people_menu = None;
        self.close_room_menu(true, false, window, cx);
        let sidebar = &mut self.appearance.sidebar;
        if hidden {
            sidebar.return_focus = return_focus;
            window.focus(&self.input.focus);
            sidebar.motion.close(instant, 160);
        } else {
            if let Some(focus) = sidebar.return_focus.take() {
                window.focus(&focus);
            }
            sidebar.motion.reopen(instant, 180);
        }
        cx.notify();
    }

    pub(super) fn sidebar_width(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Pixels {
        let sidebar = &mut self.appearance.sidebar;
        sidebar.moving = window.viewport_size().width >= px(NARROW) && sidebar.motion.animating();
        if sidebar.moving {
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }
        let scale = window.scale_factor();
        px((sidebar::WIDTH * sidebar.motion.value() * scale).round() / scale)
    }

    pub(crate) fn sidebar_grid(&self, mut grid: screen::Grid) -> screen::Grid {
        if self.appearance.sidebar.moving
            && let Some(previous) = self.input.grid
        {
            grid.cols = previous.cols;
            grid.rows = previous.rows;
        }
        grid
    }
}

use std::{path::PathBuf, sync::Arc};

use gpui::{App, FocusHandle, Global, RenderImage, ScrollHandle, Window};
use seer::Settings;

use crate::{backdrop, motion::Motion};

pub(crate) struct Glass {
    pub(crate) enabled: bool,
    pub(crate) uniform: bool,
    pub(crate) hide_lines: bool,
    pub(crate) reclaim_top: bool,
}
impl Global for Glass {}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Appearance,
    Shortcuts,
}

pub(crate) struct Appearance {
    pub(crate) section: Section,
    pub(crate) sections: [FocusHandle; 2],
    pub(crate) shortcut_focus: Vec<FocusHandle>,
    pub(crate) settings: Settings,
    pub(crate) sidebar: crate::window::sidebar_visibility::Sidebar,
    pub(crate) path: Option<PathBuf>,
    pub(crate) picture: Option<Arc<RenderImage>>,
    pub(crate) fallback: bool,
    pub(crate) error: Option<String>,
    pub(crate) motion: Option<Motion>,
    pub(crate) busy: bool,
    pub(crate) trigger: FocusHandle,
    pub(crate) close: FocusHandle,
    pub(crate) toggle: FocusHandle,
    pub(crate) uniform: FocusHandle,
    pub(crate) hide_bar: FocusHandle,
    pub(crate) hide_lines: FocusHandle,
    pub(crate) top_bar: crate::window::top_bar::TopBar,
    pub(crate) choose: FocusHandle,
    pub(crate) reset: FocusHandle,
    pub(crate) return_focus: Option<FocusHandle>,
    pub(crate) scroll: ScrollHandle,
}

impl Appearance {
    pub(crate) fn replace_picture(
        &mut self,
        picture: Option<Arc<RenderImage>>,
        window: &mut Window,
    ) {
        if let Some(previous) = std::mem::replace(&mut self.picture, picture) {
            let _ = window.drop_image(previous);
        }
    }

    pub(crate) fn load(path: Option<PathBuf>, cx: &mut App) -> Self {
        let loaded = path.as_deref().map(Settings::load_from).transpose();
        let (settings, error) = match loaded {
            Ok(settings) => (settings.unwrap_or_default(), None),
            Err(error) => (
                Settings::default(),
                Some(format!("Could not read settings: {error}")),
            ),
        };
        let mut appearance = Self {
            section: Section::Appearance,
            sections: [
                cx.focus_handle().tab_stop(true),
                cx.focus_handle().tab_stop(true),
            ],
            shortcut_focus: crate::shortcuts::SHORTCUTS
                .iter()
                .map(|_| cx.focus_handle().tab_stop(true))
                .collect(),
            sidebar: crate::window::sidebar_visibility::Sidebar::new(settings.hide_sidebar),
            settings,
            path,
            fallback: false,
            error,
            picture: None,
            motion: None,
            busy: false,
            trigger: cx.focus_handle().tab_stop(true),
            close: cx.focus_handle().tab_stop(true),
            toggle: cx.focus_handle().tab_stop(true),
            uniform: cx.focus_handle().tab_stop(true),
            hide_bar: cx.focus_handle().tab_stop(true),
            hide_lines: cx.focus_handle().tab_stop(true),
            top_bar: crate::window::top_bar::TopBar::new(),
            choose: cx.focus_handle().tab_stop(true),
            reset: cx.focus_handle().tab_stop(true),
            return_focus: None,
            scroll: ScrollHandle::new(),
        };
        if appearance.settings.glass {
            appearance.load_picture();
        }
        appearance
    }

    pub(crate) fn load_picture(&mut self) {
        let custom = self
            .settings
            .picture
            .as_deref()
            .and_then(|path| backdrop::read(path).ok());
        self.fallback = self.settings.picture.is_some() && custom.is_none();
        self.picture = custom.or_else(backdrop::default_picture);
    }

    pub(crate) fn save(&mut self, settings: Settings) -> bool {
        let result = self
            .path
            .as_deref()
            .ok_or_else(|| std::io::Error::other("Settings location is unavailable"))
            .and_then(|path| settings.save_to(path));
        match result {
            Ok(()) => {
                self.settings = settings;
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(format!("Could not save settings: {error}"));
                false
            }
        }
    }
}

pub(crate) fn fill(color: u32, cx: &App) -> gpui::Rgba {
    if let Some(glass) = cx.try_global::<Glass>().filter(|glass| glass.enabled) {
        match color {
            crate::palette::SURFACE if glass.uniform => return gpui::rgba(0),
            crate::palette::SURFACE => return gpui::rgba(0x00000066),
            crate::palette::RAISED => return gpui::rgba(0x080808f5),
            crate::palette::LINE | crate::palette::LINE_2 => return gpui::rgba(0xffffff12),
            _ => {}
        }
    }
    gpui::rgb(color)
}

pub(crate) fn separator(cx: &App) -> gpui::Rgba {
    if cx
        .try_global::<Glass>()
        .is_some_and(|glass| glass.hide_lines)
    {
        gpui::rgba(0)
    } else {
        fill(crate::palette::LINE, cx)
    }
}

pub(crate) fn reclaim_top(cx: &App) -> bool {
    cx.try_global::<Glass>()
        .is_some_and(|glass| glass.reclaim_top)
}

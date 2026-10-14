use std::{path::PathBuf, sync::Arc};

use gpui::{App, FocusHandle, Global, RenderImage, Window};
use seer::Settings;

use crate::backdrop;

pub(crate) struct Glass(pub(crate) bool);
impl Global for Glass {}

pub(crate) struct Appearance {
    pub(crate) settings: Settings,
    pub(crate) path: Option<PathBuf>,
    pub(crate) picture: Option<Arc<RenderImage>>,
    pub(crate) fallback: bool,
    pub(crate) error: Option<String>,
    pub(crate) open: bool,
    pub(crate) busy: bool,
    pub(crate) trigger: FocusHandle,
    pub(crate) back: FocusHandle,
    pub(crate) toggle: FocusHandle,
    pub(crate) choose: FocusHandle,
    pub(crate) reset: FocusHandle,
    pub(crate) return_focus: Option<FocusHandle>,
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
            settings,
            path,
            fallback: false,
            error,
            picture: None,
            open: false,
            busy: false,
            trigger: cx.focus_handle().tab_stop(true),
            back: cx.focus_handle().tab_stop(true),
            toggle: cx.focus_handle().tab_stop(true),
            choose: cx.focus_handle().tab_stop(true),
            reset: cx.focus_handle().tab_stop(true),
            return_focus: None,
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
    if cx.try_global::<Glass>().is_some_and(|glass| glass.0) {
        match color {
            crate::palette::SURFACE => return gpui::rgba(0x00000066),
            crate::palette::RAISED => return gpui::rgba(0x080808f5),
            crate::palette::LINE | crate::palette::LINE_2 => return gpui::rgba(0xffffff12),
            _ => {}
        }
    }
    gpui::rgb(color)
}

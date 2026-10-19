use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub glass: bool,
    pub uniform_glass: bool,
    pub hide_top_bar: bool,
    pub hide_sidebar: bool,
    pub hide_lines: bool,
    pub picture: Option<PathBuf>,
    pub picture_brightness: u8,
    pub glass_darkness: u8,
    pub glass_blur: u8,
    pub shortcut_overrides: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            glass: true,
            uniform_glass: true,
            hide_top_bar: true,
            hide_sidebar: false,
            hide_lines: false,
            picture: None,
            picture_brightness: 18,
            glass_darkness: 0,
            glass_blur: 12,
            shortcut_overrides: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn path() -> io::Result<PathBuf> {
        Ok(crate::store::config_dir()?.join("settings.toml"))
    }

    pub fn load_from(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(contents) => {
                let mut settings: Self = toml::from_str(&contents).map_err(io::Error::other)?;
                settings.picture_brightness = settings.picture_brightness.min(100);
                settings.glass_darkness = settings.glass_darkness.min(100);
                settings.glass_blur = settings.glass_blur.min(40);
                if !settings.glass {
                    settings.glass = true;
                    settings.picture_brightness = 0;
                    settings.glass_darkness = 100;
                    settings.glass_blur = Self::default().glass_blur;
                    settings.save_to(path)?;
                }
                Ok(settings)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let contents = toml::to_string(self).map_err(io::Error::other)?;
        crate::store::save_config(path, contents.as_bytes())
    }
}

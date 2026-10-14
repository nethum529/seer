use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub glass: bool,
    pub picture: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            glass: true,
            picture: None,
        }
    }
}

impl Settings {
    pub fn path() -> io::Result<PathBuf> {
        Ok(crate::store::config_dir()?.join("settings.toml"))
    }

    pub fn load_from(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let contents = toml::to_string(self).map_err(io::Error::other)?;
        crate::store::save_config(path, contents.as_bytes())
    }
}

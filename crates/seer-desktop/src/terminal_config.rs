use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

mod files;

#[derive(Default)]
pub(crate) struct FontSettings {
    pub(crate) families: Vec<String>,
    pub(crate) size: Option<f32>,
}

pub(crate) fn load() -> FontSettings {
    read().unwrap_or_default()
}

fn read() -> io::Result<FontSettings> {
    let home = env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let config = seer::Settings::path()
        .ok()
        .and_then(|path| path.parent()?.parent().map(Path::to_path_buf));
    let mut ghostty = Vec::new();
    if let Some(config) = &config {
        ghostty.extend([
            config.join("ghostty/config.ghostty"),
            config.join("ghostty/config"),
        ]);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = &home {
        let app = home.join("Library/Application Support/com.mitchellh.ghostty");
        ghostty.extend([app.join("config.ghostty"), app.join("config")]);
    }
    if ghostty.iter().any(|path| present(path)) {
        let mut values = Vec::new();
        for path in ghostty.into_iter().filter(|path| present(path)) {
            values.extend(files::lines(&path, true, 0)?);
        }
        return Ok(ghostty_font(values));
    }
    if let Some(path) = kitty_path(config.as_deref(), home.as_deref()) {
        return Ok(kitty_font(files::lines(&path, false, 0)?));
    }
    if let Some(path) = alacritty_path(config.as_deref(), home.as_deref()) {
        let values = files::toml(&path, 0)?;
        let font = values.get("font");
        return Ok(FontSettings {
            families: font
                .and_then(|font| font.get("normal"))
                .and_then(|normal| normal.get("family"))
                .and_then(toml::Value::as_str)
                .map(String::from)
                .into_iter()
                .collect(),
            size: font
                .and_then(|font| font.get("size"))
                .and_then(|size| {
                    size.as_float()
                        .or_else(|| size.as_integer().map(|size| size as f64))
                })
                .map(|size| size as f32),
        });
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = home {
        return iterm_font(&home.join("Library/Preferences/com.googlecode.iterm2.plist"));
    }
    Ok(FontSettings::default())
}

fn present(path: &Path) -> bool {
    !matches!(fs::symlink_metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound)
}

fn kitty_path(config: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(directory) = env::var_os("KITTY_CONFIG_DIRECTORY").filter(|value| !value.is_empty())
    {
        return Some(PathBuf::from(directory).join("kitty.conf")).filter(|path| present(path));
    }
    let directories = env::var_os("XDG_CONFIG_DIRS").unwrap_or_else(|| "/etc/xdg".into());
    config
        .map(Path::to_path_buf)
        .into_iter()
        .chain(home.map(|home| home.join(".config")))
        .chain(env::split_paths(&directories))
        .map(|path| path.join("kitty/kitty.conf"))
        .find(|path| present(path))
}

fn alacritty_path(config: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    config
        .into_iter()
        .flat_map(|config| {
            [
                config.join("alacritty/alacritty.toml"),
                config.join("alacritty.toml"),
            ]
        })
        .chain(home.into_iter().flat_map(|home| {
            [
                home.join(".config/alacritty/alacritty.toml"),
                home.join(".alacritty.toml"),
            ]
        }))
        .chain([PathBuf::from("/etc/alacritty/alacritty.toml")])
        .find(|path| present(path))
}

fn ghostty_font(values: Vec<(String, String)>) -> FontSettings {
    let mut font = FontSettings::default();
    for (key, value) in values {
        match key.as_str() {
            "font-family" if value.is_empty() => font.families.clear(),
            "font-family" => font.families.push(value),
            "font-size" => font.size = value.parse().ok(),
            _ => {}
        }
    }
    font
}

fn kitty_font(values: Vec<(String, String)>) -> FontSettings {
    let mut font = FontSettings::default();
    for (key, value) in values {
        match key.as_str() {
            "font_family" => font.families = files::font_selector(&value).into_iter().collect(),
            "font_size" => font.size = value.parse().ok(),
            _ => {}
        }
    }
    font
}

#[cfg(target_os = "macos")]
fn iterm_font(path: &Path) -> io::Result<FontSettings> {
    let value = plist::Value::from_file(path).map_err(io::Error::other)?;
    let root = value
        .as_dictionary()
        .ok_or_else(|| io::Error::other("invalid preferences"))?;
    let profiles = root.get("New Bookmarks").and_then(plist::Value::as_array);
    let Some(profiles) = profiles else {
        return Ok(FontSettings::default());
    };
    let default = root
        .get("Default Bookmark Guid")
        .and_then(plist::Value::as_string);
    let profile = if let Some(default) = default {
        profiles.iter().find(|profile| {
            profile
                .as_dictionary()
                .and_then(|profile| profile.get("Guid"))
                .and_then(plist::Value::as_string)
                == Some(default)
        })
    } else {
        profiles.first()
    };
    let description = profile
        .and_then(plist::Value::as_dictionary)
        .and_then(|profile| profile.get("Normal Font"))
        .and_then(plist::Value::as_string);
    let mut words = description.unwrap_or_default().split_whitespace();
    Ok(FontSettings {
        families: words.next().map(String::from).into_iter().collect(),
        size: words.next().and_then(|size| size.parse().ok()),
    })
}

use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub listen: SocketAddr,
    pub published_addr: String,
    #[serde(default = "default_remote")]
    pub remote: bool,
    #[serde(default = "default_state_dir")]
    pub state_dir: PathBuf,
    pub owner_name: String,
    #[serde(default)]
    pub os_users: HashMap<String, String>,
}

fn default_remote() -> bool {
    true
}

fn default_state_dir() -> PathBuf {
    state_dir_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

fn state_dir_from(
    xdg_state_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> PathBuf {
    if let Some(root) = xdg_state_home.filter(|value| !value.is_empty()) {
        return PathBuf::from(root).join("seer");
    }
    home.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/state/seer")
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let contents = fs::read_to_string(path)?;
        toml::from_str(&contents).map_err(invalid_config)
    }
}

fn invalid_config(error: toml::de::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Config;

    #[test]
    fn loads_example_config() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../broker.example.toml");
        let config = Config::load(path).expect("example config must load");

        assert_eq!(config.listen.to_string(), "127.0.0.1:7321");
        assert_eq!(config.published_addr, "seer.example.com:7321");
        assert_eq!(config.state_dir, Path::new("/var/lib/seer"));
        assert_eq!(config.owner_name, "owner");
        assert_eq!(
            config.os_users.get("owner").map(String::as_str),
            Some("owner")
        );
    }
}

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
    #[serde(default = "default_shell")]
    pub shell: String,
}

fn default_remote() -> bool {
    true
}

fn default_shell() -> String {
    "sh".to_owned()
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
    use std::io;
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
        assert_eq!(config.shell, "sh");
    }

    #[test]
    fn loads_configured_shell() {
        let config = toml::from_str::<Config>(
            "listen = \"127.0.0.1:7321\"\npublished_addr = \"host:7321\"\nowner_name = \"owner\"\nshell = \"bash\"\n",
        )
        .expect("config must load");

        assert_eq!(config.shell, "bash");
        assert!(config.state_dir.ends_with(".local/state/seer"));
    }

    #[test]
    fn selects_the_default_state_directory() {
        let xdg = super::state_dir_from(Some("/xdg".into()), Some("/home/user".into()));
        let empty_xdg = super::state_dir_from(Some("".into()), Some("/home/user".into()));
        let no_home = super::state_dir_from(None, Some("".into()));

        assert_eq!(xdg, Path::new("/xdg/seer"));
        assert_eq!(empty_xdg, Path::new("/home/user/.local/state/seer"));
        assert_eq!(no_home, Path::new("./.local/state/seer"));
    }

    #[test]
    fn rejects_invalid_config() {
        let error = toml::from_str::<Config>("listen = 1")
            .err()
            .map(super::invalid_config)
            .expect("config must be invalid");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn reports_missing_config() {
        let error = Config::load("path-that-does-not-exist")
            .err()
            .expect("missing config must return an error");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}

use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::Path;

use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub listen: SocketAddr,
    #[serde(default = "default_shell")]
    pub shell: String,
    pub users: Vec<UserConfig>,
}

#[derive(Clone, Deserialize)]
pub struct UserConfig {
    pub user: String,
    pub token: String,
}

fn default_shell() -> String {
    "sh".to_owned()
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
        assert_eq!(config.shell, "sh");
        assert_eq!(config.users.len(), 2);
        assert_eq!(config.users[0].user, "alice");
        assert_eq!(config.users[0].token, "replace-with-alice-token");
        assert_eq!(config.users[1].user, "bob");
        assert_eq!(config.users[1].token, "replace-with-bob-token");
    }

    #[test]
    fn loads_configured_shell() {
        let config =
            toml::from_str::<Config>("listen = \"127.0.0.1:7321\"\nshell = \"bash\"\nusers = []\n")
                .expect("config must load");

        assert_eq!(config.shell, "bash");
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

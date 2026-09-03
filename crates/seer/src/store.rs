use std::env;
use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const DIRECTORY_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ServerEntry {
    pub(crate) endpoint: String,
    pub(crate) alias: String,
    pub(crate) user_id: String,
    pub(crate) name: String,
    pub(crate) credential: String,
    pub(crate) current: bool,
}

#[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ServerStore {
    #[serde(default)]
    pub(crate) servers: Vec<ServerEntry>,
}

impl ServerStore {
    pub(crate) fn load() -> io::Result<Self> {
        Self::load_from(&store_path()?)
    }

    pub(crate) fn load_from(path: &Path) -> io::Result<Self> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        toml::from_str(&contents).map_err(invalid_data)
    }

    pub(crate) fn save(&self) -> io::Result<()> {
        self.save_to(&store_path()?)
    }

    pub(crate) fn save_to(&self, path: &Path) -> io::Result<()> {
        let directory = path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "store path has no directory")
        })?;
        fs::create_dir_all(directory)?;
        fs::set_permissions(directory, Permissions::from_mode(DIRECTORY_MODE))?;
        let contents = toml::to_string(self).map_err(invalid_data)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(FILE_MODE)
            .open(path)?;
        file.set_permissions(Permissions::from_mode(FILE_MODE))?;
        file.write_all(contents.as_bytes())
    }

    pub(crate) fn make_current(&mut self, entry: ServerEntry) {
        for server in &mut self.servers {
            server.current = false;
        }
        if let Some(server) = self
            .servers
            .iter_mut()
            .find(|server| server.endpoint == entry.endpoint)
        {
            *server = entry;
        } else {
            self.servers.push(entry);
        }
    }
}

fn store_path() -> io::Result<PathBuf> {
    store_path_from(env::var_os("XDG_CONFIG_HOME"), env::var_os("HOME"))
}

pub(crate) fn config_dir() -> io::Result<PathBuf> {
    config_dir_from(env::var_os("XDG_CONFIG_HOME"), env::var_os("HOME"))
}

fn config_dir_from(
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> io::Result<PathBuf> {
    let root = match xdg_config_home.filter(|value| !value.is_empty()) {
        Some(root) => PathBuf::from(root),
        None => {
            let home = home.filter(|value| !value.is_empty()).ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable")
            })?;
            PathBuf::from(home).join(".config")
        }
    };
    Ok(root.join("seer"))
}

fn store_path_from(
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> io::Result<PathBuf> {
    Ok(config_dir_from(xdg_config_home, home)?.join("servers.toml"))
}

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::ServerStore;

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn missing_store_is_empty_and_invalid_store_fails() {
        let root = short_test_directory();
        let path = root.join("servers.toml");
        assert_eq!(
            ServerStore::load_from(&path).expect("missing store must load"),
            ServerStore::default()
        );
        fs::create_dir_all(&root).expect("test directory must exist");
        fs::write(&path, "not = [valid").expect("invalid store must be written");
        assert!(ServerStore::load_from(&path).is_err());
        fs::remove_dir_all(root).expect("test directory must be removed");
    }

    fn short_test_directory() -> PathBuf {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("s2-{}-{number}", std::process::id()))
    }
}

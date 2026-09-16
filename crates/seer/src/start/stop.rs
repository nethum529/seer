use std::fs;
use std::io;
use std::path::Path;

use super::{BrokerConfig, config_dir, live_broker};
use crate::local::{process_alive, stop_pid};

pub(crate) fn stop_hosted_broker() -> io::Result<bool> {
    stop_hosted(|_| true)
}

// Issue 419: the room server on this computer can refuse the installed Seer
// after seer update. Its process is stopped by the PID that seer start saved.
pub(crate) fn stop_hosted_room(endpoint: &str) -> io::Result<bool> {
    stop_hosted(|config| serves(config, endpoint))
}

pub(crate) fn hosts_room(endpoint: &str) -> bool {
    read_config()
        .ok()
        .flatten()
        .is_some_and(|config| serves(&config, endpoint) && live_broker(&config).is_some())
}

fn serves(config: &BrokerConfig, endpoint: &str) -> bool {
    config.published_addr == endpoint || config.listen.to_string() == endpoint
}

fn stop_hosted(serves_room: impl Fn(&BrokerConfig) -> bool) -> io::Result<bool> {
    let Some(config) = read_config()? else {
        return Ok(false);
    };
    if !serves_room(&config) {
        return Ok(false);
    }
    let pid_path = config.state_dir.join("broker.pid");
    let Some(pid) = live_broker(&config) else {
        remove_pid_file(&pid_path)?;
        return Ok(false);
    };
    let stop_result = stop_pid(-pid, || process_alive(pid));
    let remove_result = remove_pid_file(&pid_path);
    stop_result?;
    remove_result?;
    Ok(true)
}

fn read_config() -> io::Result<Option<BrokerConfig>> {
    let path = config_dir()?.join("broker.toml");
    match fs::read_to_string(path) {
        Ok(contents) => toml::from_str(&contents)
            .map(Some)
            .map_err(super::invalid_data),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn remove_pid_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

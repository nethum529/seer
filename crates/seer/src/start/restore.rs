use super::{BrokerConfig, config_dir, find_broker, invalid_data, save_owner};
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

pub(super) fn remint_owner(
    config_path: &Path,
    config: &BrokerConfig,
    directory: &Path,
) -> io::Result<()> {
    let output = Command::new(find_broker()?)
        .arg(config_path)
        .arg("--remint-owner")
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "seer-broker exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let contents = String::from_utf8(output.stdout).map_err(invalid_data)?;
    let (user_id, credential) = read_owner_identity(&contents)
        .ok_or_else(|| io::Error::other("seer-broker did not write the owner identity"))?;
    save_owner(directory, config, user_id, credential)
}

pub(super) fn read_owner_identity(contents: &str) -> Option<(String, String)> {
    let user_id = contents
        .lines()
        .find_map(|line| line.strip_prefix("owner-id: "))
        .filter(|value| !value.is_empty());
    let credential = contents
        .lines()
        .find_map(|line| line.strip_prefix("owner-credential: "))
        .filter(|value| !value.is_empty());
    user_id
        .zip(credential)
        .map(|(user_id, credential)| (user_id.to_owned(), credential.to_owned()))
}

pub(crate) fn restore_owner() -> io::Result<()> {
    let directory = config_dir()?;
    let path = directory.join("broker.toml");
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let config: BrokerConfig = toml::from_str(&contents).map_err(invalid_data)?;
    let log = config.state_dir.join("broker.log");
    let (user, credential) = read_owner_identity(&fs::read_to_string(log)?).ok_or_else(|| {
        io::Error::other("Owner credential is unavailable. Restore servers.toml from backup.")
    })?;
    save_owner(&directory, &config, user, credential)
}

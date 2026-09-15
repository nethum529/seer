use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::Instant;

use super::{BrokerConfig, POLL_INTERVAL, START_TIMEOUT, config_dir, live_broker};

pub(crate) fn stop_hosted_broker() -> io::Result<bool> {
    let config = match read_config()? {
        Some(config) => config,
        None => return Ok(false),
    };
    let pid_path = config.state_dir.join("broker.pid");
    let Some(pid) = live_broker(&config) else {
        remove_pid_file(&pid_path)?;
        return Ok(false);
    };
    let stop_result = stop_process_group(pid);
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

fn stop_process_group(pid: i32) -> io::Result<()> {
    signal_process_group(pid, libc::SIGTERM)?;
    let deadline = Instant::now() + START_TIMEOUT;
    while process_exists(pid) && Instant::now() < deadline {
        thread::sleep(POLL_INTERVAL);
    }
    if process_exists(pid) {
        signal_process_group(pid, libc::SIGKILL)?;
    }
    Ok(())
}

fn signal_process_group(pid: i32, signal: i32) -> io::Result<()> {
    // Safety: kill receives the process group ID created by seer start.
    if unsafe { libc::kill(-pid, signal) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

fn process_exists(pid: i32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn remove_pid_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

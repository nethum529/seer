use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

const RELEASE_BASE_URL: &str =
    "https://github.com/nethum529/seer-releases/releases/latest/download";
const BINARIES: [&str; 3] = ["seer", "seer-broker", "seer-runtime"];

pub(crate) fn run() -> ExitCode {
    match update() {
        Ok(UpdateResult::Updated(version)) => {
            println!("Updated to {version}.");
            ExitCode::SUCCESS
        }
        Ok(UpdateResult::Current) => {
            println!("Already up to date.");
            ExitCode::SUCCESS
        }
        Err(UpdateError::Download) => {
            eprintln!("Cannot download the update. Check the internet connection.");
            ExitCode::from(1)
        }
        Err(UpdateError::Install) => {
            eprintln!("Cannot install the update.");
            ExitCode::from(1)
        }
    }
}

fn update() -> Result<UpdateResult, UpdateError> {
    let asset = asset_name().ok_or(UpdateError::Install)?;
    let base_url = std::env::var("SEER_UPDATE_URL").unwrap_or_else(|_| RELEASE_BASE_URL.to_owned());
    let download_url = format!("{}/{asset}", base_url.trim_end_matches('/'));
    let (release_url, version) = resolve_release(&download_url)?;
    let install_directory = install_directory()?;
    if version == env!("CARGO_PKG_VERSION") && desktop_is_current(&version, &install_directory) {
        return Ok(UpdateResult::Current);
    }

    let temporary_directory = TemporaryDirectory::create()?;
    let archive = temporary_directory.path().join(asset);
    download(&release_url, &archive)?;
    let unpacked = temporary_directory.path().join("unpacked");
    unpack(&archive, &unpacked)?;
    verify_release(&unpacked)?;
    install_release(&unpacked, &install_directory)?;
    Ok(UpdateResult::Updated(version))
}

fn asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("seer-linux-x86_64.tar.gz"),
        ("macos", "aarch64") => Some("seer-darwin-arm64.tar.gz"),
        ("macos", "x86_64") => Some("seer-darwin-x86_64.tar.gz"),
        _ => None,
    }
}

fn resolve_release(download_url: &str) -> Result<(String, String), UpdateError> {
    let response = ureq::AgentBuilder::new()
        .redirects(0)
        .build()
        .get(download_url)
        .call()
        .map_err(|_| UpdateError::Download)?;
    let release_url = response.header("Location").ok_or(UpdateError::Download)?;
    let version = version_from_url(release_url).ok_or(UpdateError::Download)?;
    Ok((release_url.to_owned(), version.to_owned()))
}

fn version_from_url(url: &str) -> Option<&str> {
    url.split_once("/download/")?
        .1
        .split('/')
        .next()?
        .strip_prefix('v')
        .filter(|version| !version.is_empty())
}

fn download(url: &str, destination: &Path) -> Result<(), UpdateError> {
    let response = ureq::get(url).call().map_err(|_| UpdateError::Download)?;
    let expected_size = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());
    let mut reader = response.into_reader();
    let mut file = File::create(destination).map_err(|_| UpdateError::Download)?;
    let downloaded_size = io::copy(&mut reader, &mut file).map_err(|_| UpdateError::Download)?;
    if downloaded_size == 0 || expected_size.is_some_and(|size| size != downloaded_size) {
        return Err(UpdateError::Download);
    }
    Ok(())
}

fn unpack(archive: &Path, destination: &Path) -> Result<(), UpdateError> {
    fs::create_dir(destination).map_err(|_| UpdateError::Install)?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(destination)
        .status()
        .map_err(|_| UpdateError::Install)?;
    if !status.success() {
        return Err(UpdateError::Install);
    }
    Ok(())
}

fn verify_release(directory: &Path) -> Result<(), UpdateError> {
    for binary in BINARIES.into_iter().chain(["install.sh", "version"]) {
        let metadata = fs::metadata(directory.join(binary)).map_err(|_| UpdateError::Install)?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(UpdateError::Install);
        }
    }
    Ok(())
}

fn install_directory() -> Result<PathBuf, UpdateError> {
    let executable = std::env::current_exe().map_err(|_| UpdateError::Install)?;
    let directory = executable.parent().ok_or(UpdateError::Install)?;
    if cfg!(target_os = "macos") && directory.ends_with("Contents/MacOS") {
        let home = std::env::var_os("HOME").ok_or(UpdateError::Install)?;
        return Ok(PathBuf::from(home).join(".local/bin"));
    }
    Ok(directory.to_owned())
}

fn desktop_is_current(version: &str, install_directory: &Path) -> bool {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return false;
    };
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let launcher = if cfg!(target_os = "macos") {
        home.join("Applications/Seer.app/Contents/MacOS/seer-desktop")
    } else {
        install_directory.join("seer-desktop")
    };
    launcher.is_file()
        && fs::read_to_string(data.join("seer/version"))
            .is_ok_and(|installed| installed.trim() == version)
}

fn install_release(source: &Path, install_directory: &Path) -> Result<(), UpdateError> {
    let status = Command::new("sh")
        .arg(source.join("install.sh"))
        .env("SEER_INSTALL_SOURCE", source)
        .env("SEER_BIN_DIR", install_directory)
        .env("SEER_INSTALL_NO_PATH", "1")
        .status()
        .map_err(|_| UpdateError::Install)?;
    if status.success() {
        Ok(())
    } else {
        Err(UpdateError::Install)
    }
}

fn unique_suffix() -> Result<String, UpdateError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| UpdateError::Install)?
        .as_nanos();
    Ok(format!("{}-{nanos}", std::process::id()))
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn create() -> Result<Self, UpdateError> {
        let path = std::env::temp_dir().join(format!("seer-update-{}", unique_suffix()?));
        fs::create_dir(&path).map_err(|_| UpdateError::Install)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

enum UpdateResult {
    Updated(String),
    Current,
}

enum UpdateError {
    Download,
    Install,
}

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
    let download_url = format!("{RELEASE_BASE_URL}/{asset}");
    let (release_url, version) = resolve_release(&download_url)?;
    if version == env!("CARGO_PKG_VERSION") {
        return Ok(UpdateResult::Current);
    }

    let temporary_directory = TemporaryDirectory::create()?;
    let archive = temporary_directory.path().join(asset);
    download(&release_url, &archive)?;
    let unpacked = temporary_directory.path().join("unpacked");
    unpack(&archive, &unpacked)?;
    verify_binaries(&unpacked)?;
    replace_binaries(&unpacked)?;
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

fn verify_binaries(directory: &Path) -> Result<(), UpdateError> {
    for binary in BINARIES {
        let metadata = fs::metadata(directory.join(binary)).map_err(|_| UpdateError::Install)?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(UpdateError::Install);
        }
    }
    Ok(())
}

fn replace_binaries(source_directory: &Path) -> Result<(), UpdateError> {
    let executable = std::env::current_exe().map_err(|_| UpdateError::Install)?;
    let install_directory = executable.parent().ok_or(UpdateError::Install)?;
    let suffix = unique_suffix()?;
    let mut staged = Vec::with_capacity(BINARIES.len());

    for binary in BINARIES {
        let staged_path = install_directory.join(format!(".{binary}-update-{suffix}"));
        fs::copy(source_directory.join(binary), &staged_path).map_err(|_| UpdateError::Install)?;
        staged.push((staged_path, install_directory.join(binary)));
    }

    for (staged_path, destination) in &staged {
        fs::rename(staged_path, destination).map_err(|_| UpdateError::Install)?;
    }
    Ok(())
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

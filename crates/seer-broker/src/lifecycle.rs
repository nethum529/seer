use std::ffi::c_int;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::registry::{load_json_required, random_hex, write_json_atomically};

const RECORD_VERSION: u32 = 1;
const MAX_REASON_BYTES: usize = 512;
const DIRECTORY_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;
const RECORDS_DIRECTORY: &str = "runtime-records";
const USERS_DIRECTORY: &str = "users";
const LOCK_EX: i32 = 2;
const LOCK_UN: i32 = 8;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RuntimeRecord {
    pub(crate) version: u32,
    pub(crate) user_id: String,
    pub(crate) generation: String,
    pub(crate) state: RuntimeState,
    pub(crate) state_dir: PathBuf,
    pub(crate) last_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RuntimeState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

pub(crate) struct Lifecycle {
    state_dir: PathBuf,
    records_dir: PathBuf,
}

impl Lifecycle {
    pub(crate) fn new(state_dir: impl AsRef<Path>) -> io::Result<Self> {
        install_sigterm_exit();

        fs::create_dir_all(state_dir.as_ref())?;
        fs::set_permissions(state_dir.as_ref(), Permissions::from_mode(DIRECTORY_MODE))?;
        let state_dir = fs::canonicalize(state_dir)?;
        let records_dir = state_dir.join(RECORDS_DIRECTORY);
        fs::create_dir_all(&records_dir)?;
        fs::set_permissions(&records_dir, Permissions::from_mode(DIRECTORY_MODE))?;
        Ok(Self {
            state_dir,
            records_dir,
        })
    }

    pub(crate) fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub(crate) fn user_state_dir(&self, user_id: &str) -> io::Result<PathBuf> {
        validate_user_id(user_id)?;
        Ok(self.state_dir.join(USERS_DIRECTORY).join(user_id))
    }

    pub(crate) fn lock(&self, user_id: &str) -> io::Result<RuntimeLock> {
        let path = self.lock_path(user_id)?;
        RuntimeLock::acquire(&path)
    }

    pub(crate) fn load(&self, user_id: &str) -> io::Result<Option<RuntimeRecord>> {
        let path = self.record_path(user_id)?;
        let record = match load_json_required::<RuntimeRecord>(&path) {
            Ok(record) => record,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        record.validate(user_id, &self.user_state_dir(user_id)?)?;
        Ok(Some(record))
    }

    pub(crate) fn store(&self, record: &RuntimeRecord) -> io::Result<()> {
        record.validate(&record.user_id, &self.user_state_dir(&record.user_id)?)?;
        write_json_atomically(&self.record_path(&record.user_id)?, record)
    }

    pub(crate) fn starting(&self, user_id: &str) -> io::Result<RuntimeRecord> {
        let record = RuntimeRecord {
            version: RECORD_VERSION,
            user_id: user_id.to_owned(),
            generation: random_hex::<16>()?,
            state: RuntimeState::Starting,
            state_dir: self.user_state_dir(user_id)?,
            last_reason: None,
        };
        self.store(&record)?;
        Ok(record)
    }

    pub(crate) fn mark_running_if_current(
        &self,
        user_id: &str,
        generation: &str,
    ) -> io::Result<bool> {
        let Some(mut record) = self.load(user_id)? else {
            return Ok(false);
        };
        if record.generation != generation || record.state != RuntimeState::Starting {
            return Ok(false);
        }
        record.state = RuntimeState::Running;
        record.last_reason = None;
        self.store(&record)?;
        Ok(true)
    }

    pub(crate) fn mark_failed_if_current(
        &self,
        user_id: &str,
        generation: &str,
        reason: &str,
    ) -> io::Result<bool> {
        let Some(mut record) = self.load(user_id)? else {
            return Ok(false);
        };
        if record.generation != generation {
            return Ok(false);
        }
        record.state = RuntimeState::Failed;
        record.last_reason = Some(bound_reason(reason));
        self.store(&record)?;
        Ok(true)
    }

    fn record_path(&self, user_id: &str) -> io::Result<PathBuf> {
        validate_user_id(user_id)?;
        Ok(self.records_dir.join(format!("{user_id}.json")))
    }

    fn lock_path(&self, user_id: &str) -> io::Result<PathBuf> {
        validate_user_id(user_id)?;
        Ok(self.records_dir.join(format!("{user_id}.lock")))
    }
}

impl RuntimeRecord {
    fn validate(&self, user_id: &str, expected_state_dir: &Path) -> io::Result<()> {
        if self.version != RECORD_VERSION {
            return Err(invalid_record("runtime record version is unsupported"));
        }
        if self.user_id != user_id || self.state_dir != expected_state_dir {
            return Err(invalid_record("runtime record identity is invalid"));
        }
        if !valid_generation(&self.generation) {
            return Err(invalid_record("runtime record generation is invalid"));
        }
        if self
            .last_reason
            .as_ref()
            .is_some_and(|reason| reason.len() > MAX_REASON_BYTES)
        {
            return Err(invalid_record("runtime record reason is too long"));
        }
        if matches!(self.state, RuntimeState::Starting | RuntimeState::Running)
            && self.last_reason.is_some()
        {
            return Err(invalid_record("runtime record has an invalid reason"));
        }
        if self.state == RuntimeState::Failed && self.last_reason.is_none() {
            return Err(invalid_record("failed runtime record has no reason"));
        }
        Ok(())
    }
}

pub(crate) struct RuntimeLock {
    file: File,
}

impl RuntimeLock {
    fn acquire(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .mode(FILE_MODE)
            .open(path)?;
        file.set_permissions(Permissions::from_mode(FILE_MODE))?;
        lock_file(&file)?;
        Ok(Self { file })
    }
}

impl Drop for RuntimeLock {
    fn drop(&mut self) {
        // SAFETY: the file descriptor is owned by this guard and remains valid during drop.
        unsafe {
            flock(self.file.as_raw_fd(), LOCK_UN);
        }
    }
}

fn lock_file(file: &File) -> io::Result<()> {
    loop {
        // SAFETY: flock only reads the owned file descriptor and changes its advisory lock.
        let result = unsafe { flock(file.as_raw_fd(), LOCK_EX) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

pub(crate) fn validate_user_id(user_id: &str) -> io::Result<()> {
    let valid = !user_id.is_empty()
        && user_id.len() <= 64
        && user_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime user ID is unsafe",
        ))
    }
}

fn valid_generation(generation: &str) -> bool {
    generation.len() == 32
        && generation
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn bound_reason(reason: &str) -> String {
    let mut end = reason.len().min(MAX_REASON_BYTES);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason[..end].to_owned()
}

fn install_sigterm_exit() {
    // SAFETY: `signal` installs a valid handler before broker connections start.
    unsafe {
        signal(15, exit_on_sigterm as *const () as usize);
    }
}

extern "C" fn exit_on_sigterm(_signal: c_int) {
    // SAFETY: `_exit` is async-signal-safe and ends the broker process.
    unsafe {
        _exit(0);
    }
}

fn invalid_record(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

// SAFETY: these declarations match the Linux and macOS libc ABI.
unsafe extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
    fn signal(signal: c_int, handler: usize) -> usize;
    fn _exit(status: c_int) -> !;
}

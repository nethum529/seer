use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static NEXT_TEMPORARY_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn temporary_directory(prefix: &str) -> PathBuf {
    let counter = NEXT_TEMPORARY_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after the Unix epoch")
        .as_nanos()
        % 1_000_000_000;
    let path = PathBuf::from(format!(
        "/tmp/{prefix}-{}-{counter}-{timestamp}",
        std::process::id()
    ));
    fs::create_dir(&path).expect("temporary directory must be created");
    path
}

pub(crate) fn remove_directory(path: &Path, message: &str) {
    let mut last_error = None;
    for attempt in 0..20 {
        match fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => last_error = Some(error),
        }
        if attempt < 19 {
            thread::sleep(Duration::from_millis(50));
        }
    }
    panic!("{message}: {last_error:?}");
}

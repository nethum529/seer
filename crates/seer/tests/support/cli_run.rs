use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[path = "runtimes.rs"]
mod runtimes;

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct TestConfig {
    pub(crate) root: PathBuf,
}

impl TestConfig {
    pub(crate) fn new() -> Self {
        let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root = Path::new("/tmp").join(format!("s2-{}-{number}", std::process::id()));
        fs::create_dir_all(&root).expect("test directory must exist");
        Self { root }
    }
}

impl Drop for TestConfig {
    fn drop(&mut self) {
        let _ = Command::new(env!("CARGO_BIN_EXE_seer"))
            .arg("stop")
            .env("XDG_CONFIG_HOME", &self.root)
            .env("XDG_STATE_HOME", self.root.join("state-home"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        runtimes::stop_runtimes(&self.root);
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn run(config: &TestConfig, arguments: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_seer"))
        .args(arguments)
        .env("XDG_CONFIG_HOME", &config.root)
        .env("XDG_STATE_HOME", config.root.join("state-home"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("seer must start");
    child
        .stdin
        .take()
        .expect("stdin must be piped")
        .write_all(input.as_bytes())
        .expect("input must be written");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child
            .try_wait()
            .expect("seer status must be available")
            .is_some()
        {
            return child.wait_with_output().expect("seer output must be read");
        }
        if Instant::now() >= deadline {
            child.kill().expect("seer must be killed after timeout");
            let _ = child.wait();
            panic!("seer did not finish before timeout");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("output must be UTF-8")
}

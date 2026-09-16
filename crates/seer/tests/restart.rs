#![cfg(target_os = "linux")]
use std::fs;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[path = "support/cli_run.rs"]
mod cli_run;
#[path = "support/process.rs"]
mod process;

use cli_run::{TestConfig, run, text};
use process::{process_exists, wait_for_process_end};

const USER: &str = "restart-user-id";
const WAIT: Duration = Duration::from_secs(20);

// Issue 420: seer restart ends the runtimes of this person here, the open
// window says why it closed, and the next window starts the installed runtime.
#[test]
fn restart_moves_the_terminals_to_the_installed_runtime() {
    let config = TestConfig::new();
    let store = config.root.join("seer/servers.toml");
    fs::create_dir_all(config.root.join("seer")).expect("config directory must exist");
    fs::write(
        &store,
        format!(
            "[[servers]]\nendpoint = \"127.0.0.1:1\"\nalias = \"room\"\nuser_id = \"{USER}\"\nname = \"alice\"\ncredential = \"secret\"\ncurrent = true\n"
        ),
    )
    .expect("server store must be written");
    let saved = fs::read(&store).expect("server store must be read");

    let nothing = run(&config, &["restart", "--yes"], "");
    assert_eq!(nothing.status.code(), Some(0), "{}", text(&nothing.stderr));
    assert_eq!(
        text(&nothing.stdout),
        "No Seer terminals run on this computer.\n"
    );

    let window = open_window(&config.root);
    let old = wait_for_runtime(&config.root, None);

    let refused = run(&config, &["restart"], "y\n");
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        text(&refused.stderr).contains("--yes"),
        "{}",
        text(&refused.stderr)
    );
    assert!(process_exists(old));

    let restarted = run(&config, &["restart", "--yes"], "");
    assert_eq!(
        restarted.status.code(),
        Some(0),
        "{}",
        text(&restarted.stderr)
    );
    wait_for_process_end(old);
    let closed = window
        .wait_with_output()
        .expect("window output must be read");
    assert!(
        text(&closed.stdout)
            .contains("Your terminals were restarted. Open Seer again to continue."),
        "{}",
        String::from_utf8_lossy(&closed.stdout)
    );

    let mut next = open_window(&config.root);
    let new = wait_for_runtime(&config.root, Some(old));
    assert_ne!(new, old);
    assert_eq!(
        fs::read_link(format!("/proc/{new}/exe")).expect("runtime binary must be readable"),
        Path::new(env!("CARGO_BIN_EXE_seer-runtime"))
    );
    assert_eq!(fs::read(&store).expect("server store must be read"), saved);
    // SAFETY: the window runs in a private process group that this test started.
    unsafe {
        libc::kill(-(next.id() as i32), libc::SIGKILL);
    }
    let _ = next.wait();
}

fn open_window(root: &Path) -> Child {
    Command::new("script")
        .process_group(0)
        .args([
            "-qec",
            &format!("{} attach", env!("CARGO_BIN_EXE_seer")),
            "/dev/null",
        ])
        .env("XDG_CONFIG_HOME", root)
        .env("XDG_STATE_HOME", root.join("state-home"))
        .env("TERM", "xterm")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("a window must start")
}

fn wait_for_runtime(root: &Path, old: Option<i32>) -> i32 {
    let directory = root.join("state-home/seer/runtimes").join(USER);
    let deadline = Instant::now() + WAIT;
    loop {
        let pid = fs::read_to_string(directory.join("runtime.pid"))
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .filter(|pid| Some(*pid) != old && process_exists(*pid));
        if let Some(pid) = pid
            && has_window(&directory.join("socket"))
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "a runtime with a window must run"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn has_window(socket: &Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(WAIT));
    if !matches!(
        codec::decode(&mut stream),
        Ok(ServerMsg::RuntimeReady { .. })
    ) || codec::encode(&mut stream, &ClientMsg::QueryStatus).is_err()
    {
        return false;
    }
    matches!(
        codec::decode(&mut stream),
        Ok(ServerMsg::Status { windows: Some(windows), .. }) if !windows.is_empty()
    )
}

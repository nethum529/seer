use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

// Tests point the room at a dead address, so seer stop never reaches the
// local runtime. Windows go first: a window still in its startup path would
// start a new runtime after the old one stopped.
pub(crate) fn stop_runtimes(root: &Path) {
    let state_home = format!("XDG_STATE_HOME={}", root.join("state-home").display());
    let mut runtimes = Vec::new();
    for pid in processes_with_environment(&state_home) {
        if is_window(pid) {
            signal_and_wait(pid, libc::SIGKILL);
        } else {
            runtimes.push(pid);
        }
    }
    let Ok(users) = fs::read_dir(root.join("state-home/seer/runtimes")) else {
        return;
    };
    for user in users.flatten() {
        let pid = fs::read_to_string(user.path().join("runtime.pid"))
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok());
        runtimes.extend(pid);
    }
    for pid in runtimes {
        signal_and_wait(pid, libc::SIGTERM);
    }
}

// Linux only. Other systems fall back to the recorded PID alone.
fn processes_with_environment(entry: &str) -> Vec<i32> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|process| process.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| {
            fs::read(format!("/proc/{pid}/environ")).is_ok_and(|environ| {
                environ
                    .split(|byte| *byte == 0)
                    .any(|variable| variable == entry.as_bytes())
            })
        })
        .collect()
}

fn is_window(pid: i32) -> bool {
    fs::read_link(format!("/proc/{pid}/exe"))
        .is_ok_and(|exe| exe == Path::new(env!("CARGO_BIN_EXE_seer")))
}

fn signal_and_wait(pid: i32, signal: i32) {
    // SAFETY: kill takes a PID this test found under its own root and a signal number.
    unsafe {
        libc::kill(pid, signal);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    // SAFETY: kill with signal 0 only checks that the process exists.
    while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
}

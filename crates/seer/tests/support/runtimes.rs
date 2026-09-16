use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

// Tests point the room at a dead address, so seer stop never reaches the
// local runtime. The runtime must go by the PID it recorded.
pub(crate) fn stop_runtimes(root: &Path) {
    let Ok(users) = fs::read_dir(root.join("state-home/seer/runtimes")) else {
        return;
    };
    for user in users.flatten() {
        let pid = fs::read_to_string(user.path().join("runtime.pid"))
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok());
        let Some(pid) = pid else {
            continue;
        };
        // SAFETY: kill takes a PID this test recorded and a signal number.
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        // SAFETY: kill with signal 0 only checks that the process exists.
        while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(25));
        }
    }
}

#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "macos")]
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) fn process_exists(pid: i32) -> bool {
    #[cfg(target_os = "linux")]
    {
        fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .is_some_and(|stat| stat.split_whitespace().nth(2) != Some("Z"))
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output()
            .ok()
            .is_some_and(|output| {
                output.status.success()
                    && !String::from_utf8_lossy(&output.stdout).trim().starts_with('Z')
            })
    }
}

pub(crate) fn wait_for_process_end(pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while process_exists(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!process_exists(pid), "process must stop");
}

use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use super::support::{TestFiles, wait_for_file};

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

impl TestFiles {
    pub(crate) fn runtime_record(&self, user: &str) -> serde_json::Value {
        let path = self.state_dir.join(format!("runtime-records/{user}.json"));
        assert!(wait_for_file(&path));
        serde_json::from_str(&fs::read_to_string(path).expect("runtime record must be readable"))
            .expect("runtime record must be valid JSON")
    }

    pub(crate) fn wait_for_runtime_state(&self, user: &str, expected: &str) -> serde_json::Value {
        let path = self.state_dir.join(format!("runtime-records/{user}.json"));
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while Instant::now() < deadline {
            if path.is_file()
                && let Ok(record) = fs::read_to_string(&path)
                && let Ok(record) = serde_json::from_str::<serde_json::Value>(&record)
                && record["state"] == expected
            {
                return record;
            }
            thread::sleep(POLL_INTERVAL);
        }
        panic!("runtime did not reach state {expected}");
    }
}

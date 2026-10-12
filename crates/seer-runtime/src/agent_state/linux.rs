use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::Read;

use super::{Agent, agent_arguments, identify, name};

pub(super) fn group_agent(group: i32, shell_pid: Option<u32>) -> Option<Agent> {
    let mut pending = VecDeque::from([group]);
    if let Some(pid) = shell_pid.and_then(|pid| i32::try_from(pid).ok()) {
        pending.push_back(pid);
    }
    let mut seen = HashSet::new();
    let mut budget = Budget {
        tasks: 128,
        bytes: 64 * 1024,
    };
    while seen.len() < 128 {
        let pid = pending.pop_front()?;
        if !seen.insert(pid) {
            continue;
        }
        if process_group(pid) == Some(group) {
            let name = name(pid);
            if let Some(agent) = identify(&name) {
                return Some(agent);
            }
            if matches!(name.as_str(), "node" | "bun") {
                let args = read_limited(&format!("/proc/{pid}/cmdline"), 64 * 1024);
                if let Some(agent) = args.as_deref().and_then(agent_arguments) {
                    return Some(agent);
                }
            }
        }
        budget.children(pid, &mut pending);
    }
    None
}

fn process_group(pid: i32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(2)?
        .parse()
        .ok()
}

struct Budget {
    tasks: usize,
    bytes: usize,
}

impl Budget {
    fn children(&mut self, pid: i32, pending: &mut VecDeque<i32>) {
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
            return;
        };
        for task in tasks.take(self.tasks) {
            self.tasks -= 1;
            if self.bytes == 0 || pending.len() >= 128 {
                break;
            }
            let Ok(task) = task else { continue };
            let path = task.path().join("children");
            let Some(bytes) = path
                .to_str()
                .and_then(|path| read_limited(path, self.bytes))
            else {
                continue;
            };
            self.bytes -= bytes.len();
            let end = bytes
                .iter()
                .rposition(u8::is_ascii_whitespace)
                .map_or(0, |at| at + 1);
            let children = String::from_utf8_lossy(&bytes[..end]);
            pending.extend(
                children
                    .split_whitespace()
                    .filter_map(|pid| pid.parse::<i32>().ok())
                    .filter(|pid| *pid > 0)
                    .take(128 - pending.len()),
            );
        }
    }
}

fn read_limited(path: &str, limit: usize) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(limit as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;

use super::rules::Agent;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod linux;

pub(super) struct Job {
    pub(super) group: i32,
    pub(super) name: String,
    pub(super) agent: Option<Agent>,
}

pub(super) fn observe(descriptor: &OwnedFd, shell_pid: Option<u32>) -> Job {
    // SAFETY: descriptor owns an open master PTY for the duration of this call.
    let group = unsafe { libc::tcgetpgrp(descriptor.as_raw_fd()) };
    let name = if group > 0 {
        name(group)
    } else {
        String::new()
    };
    let agent = identify(&name).or_else(|| {
        (group > 0 && Some(group as u32) != shell_pid)
            .then(|| group_agents(group, shell_pid))
            .flatten()
    });
    Job { group, name, agent }
}

fn identify(name: &str) -> Option<Agent> {
    match name {
        "claude" | "claude-code" => Some(Agent::Claude),
        "codex" => Some(Agent::Codex),
        _ => None,
    }
}

fn agent_arguments(arguments: &[u8]) -> Option<Agent> {
    let mut args = arguments
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty());
    let executable = std::str::from_utf8(args.next()?).ok()?;
    let basename = Path::new(executable).file_name()?.to_str()?;
    if let Some(agent) = identify(basename) {
        return Some(agent);
    }
    if !matches!(basename, "node" | "bun") {
        return None;
    }
    let script = std::str::from_utf8(args.next()?).ok()?;
    let basename = Path::new(script).file_name()?.to_str()?;
    identify(basename).or_else(|| {
        if script.ends_with("/@anthropic-ai/claude-code/cli.js") {
            Some(Agent::Claude)
        } else if script.ends_with("/@openai/codex/bin/codex.js") {
            Some(Agent::Codex)
        } else {
            None
        }
    })
}

#[cfg(target_os = "linux")]
fn name(pid: i32) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|name| name.trim().to_owned())
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn group_agents(group: i32, shell_pid: Option<u32>) -> Option<Agent> {
    linux::group_agent(group, shell_pid)
}

#[cfg(target_os = "macos")]
fn name(pid: i32) -> String {
    let mut bytes = [0u8; 256];
    // SAFETY: bytes is writable for its full length and proc_name does not retain it.
    let count = unsafe { libc::proc_name(pid, bytes.as_mut_ptr().cast(), bytes.len() as u32) };
    if count <= 0 {
        return String::new();
    }
    String::from_utf8_lossy(&bytes[..count as usize])
        .trim_end_matches('\0')
        .to_owned()
}

#[cfg(target_os = "macos")]
fn group_agents(group: i32, _shell_pid: Option<u32>) -> Option<Agent> {
    // PROC_PGRP_ONLY selects members of this PTY's foreground job.
    const PROC_PGRP_ONLY: u32 = 2;
    let mut pids = [0i32; 128];
    // SAFETY: pids has room for the byte count passed and remains alive during the call.
    let count = unsafe {
        libc::proc_listpids(
            PROC_PGRP_ONLY,
            group as u32,
            pids.as_mut_ptr().cast(),
            std::mem::size_of_val(&pids) as i32,
        )
    };
    if count <= 0 {
        return None;
    }
    for pid in pids
        .into_iter()
        .take((count as usize / 4).min(128))
        .filter(|pid| *pid > 0)
    {
        let name = name(pid);
        if let Some(agent) = identify(&name) {
            return Some(agent);
        }
        if let Some(agent) = matches!(name.as_str(), "node" | "bun")
            .then(|| arguments(pid).and_then(|args| agent_arguments(&args)))
            .flatten()
        {
            return Some(agent);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn arguments(pid: i32) -> Option<Vec<u8>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut buffer = vec![0u8; 64 * 1024];
    let mut length = buffer.len();
    // SAFETY: mib and buffer are valid for their lengths, and sysctl does not retain either.
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buffer.as_mut_ptr().cast(),
            &raw mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || length < 4 {
        return None;
    }
    buffer.truncate(length);
    let executable_end = buffer[4..].iter().position(|byte| *byte == 0)? + 4;
    let start = buffer[executable_end..]
        .iter()
        .position(|byte| *byte != 0)?
        + executable_end;
    Some(buffer[start..].to_vec())
}

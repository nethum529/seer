use std::collections::BTreeSet;
use std::io;

pub(crate) fn programs(shell_pid: Option<u32>, shell: &str) -> io::Result<Vec<String>> {
    let shell_pid = shell_pid
        .and_then(|pid| i32::try_from(pid).ok())
        .ok_or_else(|| io::Error::other("cannot inspect the terminal shell"))?;
    // SAFETY: getsid takes a process ID and does not use pointers.
    let session = unsafe { libc::getsid(shell_pid) };
    if session < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut programs = BTreeSet::new();
    for pid in pids()? {
        // SAFETY: getsid takes a process ID and does not use pointers.
        if unsafe { libc::getsid(pid) } != session {
            continue;
        }
        let Some(name) = live_name(pid)? else {
            continue;
        };
        if pid != shell_pid || !(name == shell || super::is_shell(&name)) {
            programs.insert(name);
        }
    }
    Ok(programs.into_iter().collect())
}

#[cfg(target_os = "linux")]
fn pids() -> io::Result<Vec<i32>> {
    std::fs::read_dir("/proc")?
        .map(|entry| {
            Ok(entry?
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<i32>().ok()))
        })
        .filter_map(|result| result.transpose())
        .collect()
}

#[cfg(target_os = "linux")]
fn live_name(pid: i32) -> io::Result<Option<String>> {
    let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let Some((_, fields)) = stat.rsplit_once(')') else {
        return Err(io::Error::other("cannot read terminal process state"));
    };
    if matches!(fields.split_whitespace().next(), Some("Z" | "X")) {
        return Ok(None);
    }
    Ok(Some(program_name(pid)))
}

#[cfg(target_os = "macos")]
fn pids() -> io::Result<Vec<i32>> {
    // PROC_ALL_PIDS includes background jobs in every process group.
    const PROC_ALL_PIDS: u32 = 1;
    let mut pids = vec![0i32; 256];
    loop {
        let bytes = i32::try_from(std::mem::size_of_val(pids.as_slice()))
            .map_err(|_| io::Error::other("too many terminal processes"))?;
        // SAFETY: pids is writable for bytes bytes and lives for the call.
        let count =
            unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, pids.as_mut_ptr().cast(), bytes) };
        if count <= 0 {
            return Err(io::Error::last_os_error());
        }
        if count < bytes {
            pids.truncate(count as usize / std::mem::size_of::<i32>());
            pids.retain(|pid| *pid > 0);
            return Ok(pids);
        }
        pids.resize(pids.len() * 2, 0);
    }
}

#[cfg(target_os = "macos")]
fn live_name(pid: i32) -> io::Result<Option<String>> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let bytes = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: info has space for bytes bytes and proc_pidinfo initializes it on success.
    let count = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            bytes,
        )
    };
    if count != bytes {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    // SAFETY: proc_pidinfo wrote all bytes of info.
    if unsafe { info.assume_init() }.pbi_status == libc::SZOMB {
        return Ok(None);
    }
    Ok(Some(program_name(pid)))
}

fn program_name(pid: i32) -> String {
    let name = super::name(pid);
    if name.is_empty() {
        format!("process {pid}")
    } else {
        super::identify(&name).map_or(name, |agent| agent.name().into())
    }
}

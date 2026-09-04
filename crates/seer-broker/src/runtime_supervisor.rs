use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::Duration;

use crate::lifecycle::Lifecycle;
use crate::runtime::{ProcessTable, RuntimeProcess, UserLocks};

const SUPERVISOR_INTERVAL: Duration = Duration::from_millis(250);

pub(super) fn spawn_supervisor(
    processes: Weak<Mutex<ProcessTable>>,
    lifecycle: Weak<Lifecycle>,
    user_locks: Weak<Mutex<UserLocks>>,
) {
    let _monitor = thread::spawn(move || {
        supervise_runtimes(processes, lifecycle, user_locks);
    });
}

fn supervise_runtimes(
    processes: Weak<Mutex<ProcessTable>>,
    lifecycle: Weak<Lifecycle>,
    user_locks: Weak<Mutex<UserLocks>>,
) {
    loop {
        thread::sleep(SUPERVISOR_INTERVAL);
        let Some(processes) = processes.upgrade() else {
            return;
        };
        let exited = {
            let Ok(mut processes) = processes.lock() else {
                eprintln!("runtime supervisor lock failed");
                return;
            };
            let mut exited = Vec::new();
            for (user_id, process) in processes.iter_mut() {
                match process.child.try_wait() {
                    Ok(Some(status)) => exited.push((
                        user_id.clone(),
                        process.generation.clone(),
                        process.socket_path.clone(),
                        format!("runtime exited with status {status}"),
                    )),
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!("runtime status check failed for user {user_id}: {error}");
                    }
                }
            }
            for (user_id, generation, _, _) in &exited {
                if processes
                    .get(user_id)
                    .is_some_and(|process| process.generation == *generation)
                {
                    processes.remove(user_id);
                }
            }
            exited
        };
        for (user_id, generation, socket_path, reason) in exited {
            let Some(lifecycle) = lifecycle.upgrade() else {
                return;
            };
            let Some(user_locks) = user_locks.upgrade() else {
                return;
            };
            if let Err(error) = mark_exited(
                &lifecycle,
                &user_locks,
                &user_id,
                &generation,
                &socket_path,
                &reason,
            ) {
                eprintln!("runtime exit persistence failed for user {user_id}: {error}");
            }
        }
    }
}

fn mark_exited(
    lifecycle: &Lifecycle,
    user_locks: &Mutex<UserLocks>,
    user_id: &str,
    generation: &str,
    socket_path: &Path,
    reason: &str,
) -> io::Result<()> {
    let user_lock = {
        let mut locks = user_locks
            .lock()
            .map_err(|_| io::Error::other("runtime user locks are poisoned"))?;
        Arc::clone(
            locks
                .entry(user_id.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    };
    let _user_guard = user_lock
        .lock()
        .map_err(|_| io::Error::other("runtime user lock is poisoned"))?;
    let _file_lock = lifecycle.lock(user_id)?;
    if lifecycle.mark_failed_if_current(user_id, generation, reason)? {
        remove_runtime_socket(socket_path)?;
    }
    Ok(())
}

pub(super) fn terminate_process(mut process: RuntimeProcess) -> io::Result<()> {
    if process.child.try_wait()?.is_some() {
        return Ok(());
    }
    process.child.kill()?;
    process.child.wait()?;
    Ok(())
}

pub(super) fn remove_runtime_socket(path: &Path) -> io::Result<()> {
    if !path.exists() || UnixStream::connect(path).is_ok() {
        return Ok(());
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

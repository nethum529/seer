use std::io::{self, IsTerminal};
use std::time::Duration;

use seer_core::proto::{ClientMsg, ServerMsg, codec};

use super::CommandError;
use super::ps::stop_runtime;
use crate::processes::{self, Detail, Kind, Process};

// The runtime answers after its windows close, and waits at most 2 s for them.
const NOTICE_TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) fn restart(yes: bool) -> Result<(), CommandError> {
    let state_home = crate::local::state_dir()
        .map_err(CommandError::system)?
        .parent()
        .map(std::path::Path::to_path_buf);
    let processes = processes::list().map_err(CommandError::system)?;
    let runtimes: Vec<&Process> = processes
        .iter()
        .filter(|process| process.kind == Kind::Runtime && process.state_home == state_home)
        .collect();
    if runtimes.is_empty() {
        println!("No Seer terminals run on this computer.");
        return Ok(());
    }
    if !yes && !confirmed()? {
        return Ok(());
    }
    for runtime in runtimes {
        if let Detail::Runtime { socket, .. } = &runtime.detail {
            notify_windows(socket);
        }
        stop_runtime(&processes, runtime.pid())?;
    }
    Ok(())
}

fn confirmed() -> Result<bool, CommandError> {
    if !io::stdin().is_terminal() {
        return Err(CommandError::usage(
            "seer restart needs a terminal to ask. Run seer restart --yes to restart without a question.",
        ));
    }
    println!(
        "The shells in your Seer terminals on this computer will end. Your tabs and panes come back when you open Seer again."
    );
    let answer = crate::prompt::visible("Continue? [y/N] ").map_err(CommandError::system)?;
    Ok(matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
}

// An older runtime refuses the message. Its windows then say the server stopped.
fn notify_windows(socket: &std::path::Path) {
    let Ok(mut stream) = crate::local::connect_within(socket, None, NOTICE_TIMEOUT) else {
        return;
    };
    if stream.set_read_timeout(Some(NOTICE_TIMEOUT)).is_ok()
        && codec::encode(&mut stream, &ClientMsg::Stop).is_ok()
    {
        let _: io::Result<ServerMsg> = codec::decode(&mut stream);
    }
}

pub(super) fn print_restarted() {
    println!("Your terminals were restarted. Open Seer again to continue.");
}

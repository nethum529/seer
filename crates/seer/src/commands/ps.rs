use std::path::Path;

use super::{CommandError, print_columns};
use crate::local;
use crate::processes::{self, Detail, Kind, Process, RuntimeStatus};

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PsAction {
    List,
    Clean,
    Stop(u32),
}

pub(crate) fn ps(action: PsAction) -> Result<(), CommandError> {
    let processes = processes::list().map_err(CommandError::system)?;
    match action {
        PsAction::List => {
            print_list(&processes);
            Ok(())
        }
        PsAction::Clean => clean(&processes),
        PsAction::Stop(pid) => stop_runtime(&processes, pid),
    }
}

fn print_list(processes: &[Process]) {
    let rows: Vec<Vec<String>> = processes
        .iter()
        .map(|process| {
            vec![
                process.pid.to_string(),
                process.kind.name().to_owned(),
                format_up(process.up_secs),
                format!("{}%", process.cpu),
                describe(&process.detail),
                process
                    .state_home
                    .as_deref()
                    .map_or_else(|| "-".to_owned(), |home| home.display().to_string()),
            ]
        })
        .collect();
    print_columns(&["PID", "KIND", "UP", "CPU", "DETAIL", "STATE HOME"], &rows);
}

fn format_up(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let clock = format!(
        "{:02}:{:02}:{:02}",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    );
    if days > 0 {
        format!("{days}d {clock}")
    } else {
        clock
    }
}

fn describe(detail: &Detail) -> String {
    match detail {
        Detail::Broker => "-".to_owned(),
        Detail::Runtime {
            status: None,
            socket,
        } if !socket.exists() => "no socket".to_owned(),
        Detail::Runtime { status: None, .. } => "no answer".to_owned(),
        Detail::Runtime {
            status: Some(RuntimeStatus::Older),
            ..
        } => "older runtime, no counts".to_owned(),
        Detail::Runtime {
            status: Some(RuntimeStatus::Counts { windows, shells }),
            ..
        } => format!(
            "{}, {}",
            count(windows.len(), "window"),
            count(*shells as usize, "shell")
        ),
        Detail::Window { terminal: None } => "no terminal".to_owned(),
        Detail::Window {
            terminal: Some(tty),
        } => format!("terminal {tty}"),
    }
}

fn count(number: usize, noun: &str) -> String {
    if number == 1 {
        format!("1 {noun}")
    } else {
        format!("{number} {noun}s")
    }
}

fn clean(processes: &[Process]) -> Result<(), CommandError> {
    let stuck: Vec<&Process> = processes
        .iter()
        .filter(|process| process.kind == Kind::Window && !process.has_terminal())
        .collect();
    for window in &stuck {
        let pid = to_signed(window.pid)?;
        local::stop_pid(pid, || local::process_alive(pid)).map_err(CommandError::system)?;
    }
    if stuck.is_empty() {
        println!("No window without a terminal.");
    } else {
        println!(
            "Stopped {} without a terminal.",
            count(stuck.len(), "window")
        );
    }
    Ok(())
}

fn stop_runtime(processes: &[Process], pid: u32) -> Result<(), CommandError> {
    let Some(process) = processes.iter().find(|process| process.pid == pid) else {
        return Err(CommandError::usage(format!(
            "PID {pid} is not a Seer process of yours on this computer. Run seer ps."
        )));
    };
    let Detail::Runtime { socket, .. } = &process.detail else {
        return Err(CommandError::usage(format!(
            "PID {pid} is a {}, not a runtime. seer ps --stop stops only a runtime.",
            process.kind.name()
        )));
    };
    let alive = |socket: &Path| local::connect(socket, None).is_ok();
    local::stop_pid(to_signed(pid)?, || alive(socket)).map_err(CommandError::system)?;
    println!("Runtime {pid} stopped with its shells.");
    Ok(())
}

fn to_signed(pid: u32) -> Result<i32, CommandError> {
    i32::try_from(pid).map_err(CommandError::system)
}

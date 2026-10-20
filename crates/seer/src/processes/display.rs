use super::{Detail, RuntimeStatus};
use crate::commands::ps::count;

pub(super) fn format_up(secs: u64) -> String {
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

pub(super) fn describe(detail: &Detail) -> String {
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

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use seer_core::proto::{ClientMsg, ServerMsg, codec};

const STATUS_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Broker,
    Runtime,
    Window,
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Broker => "broker",
            Kind::Runtime => "runtime",
            Kind::Window => "window",
        }
    }
}

pub(crate) enum RuntimeStatus {
    Counts { windows: Vec<u32>, shells: u32 },
    Older,
}

pub(crate) enum Detail {
    Broker,
    Runtime {
        socket: PathBuf,
        status: Option<RuntimeStatus>,
    },
    Window {
        terminal: Option<String>,
    },
}

pub(crate) struct Process {
    pub(crate) pid: u32,
    pub(crate) kind: Kind,
    pub(crate) up_secs: u64,
    pub(crate) cpu: String,
    pub(crate) state_home: Option<PathBuf>,
    pub(crate) detail: Detail,
}

impl Process {
    pub(crate) fn has_terminal(&self) -> bool {
        matches!(&self.detail, Detail::Window { terminal: Some(_) })
    }
}

// A window belongs to the state home of the runtime that reports it attached.
pub(crate) fn list() -> io::Result<Vec<Process>> {
    let own_pid = std::process::id();
    let mut processes: Vec<Process> = ps_rows()?
        .into_iter()
        .filter(|row| row.pid != own_pid)
        .filter_map(classify)
        .collect();
    let homes: Vec<(u32, PathBuf)> = processes
        .iter()
        .filter_map(|process| match (&process.detail, &process.state_home) {
            (
                Detail::Runtime {
                    status: Some(RuntimeStatus::Counts { windows, .. }),
                    ..
                },
                Some(home),
            ) => Some(windows.iter().map(|pid| (*pid, home.clone()))),
            _ => None,
        })
        .flatten()
        .collect();
    for process in &mut processes {
        if process.kind == Kind::Window {
            process.state_home = homes
                .iter()
                .find(|(pid, _)| *pid == process.pid)
                .map(|(_, home)| home.clone());
        }
    }
    processes.sort_by_key(|process| (process.kind, process.pid));
    Ok(processes)
}

struct PsRow {
    pid: u32,
    up_secs: u64,
    cpu: String,
    tty: Option<String>,
    args: Vec<String>,
}

fn ps_rows() -> io::Result<Vec<PsRow>> {
    // SAFETY: getuid takes no arguments and cannot fail.
    let uid = unsafe { libc::getuid() };
    let output = Command::new("ps")
        .args(["-o", "pid=,etime=,pcpu=,tty=,args=", "-U", &uid.to_string()])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("ps did not list the processes"));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_row)
        .collect())
}

fn parse_row(line: &str) -> Option<PsRow> {
    let mut fields = line.split_whitespace();
    let pid = fields.next()?.parse().ok()?;
    let up_secs = parse_elapsed(fields.next()?)?;
    let cpu = fields.next()?.to_owned();
    let tty = fields.next().filter(|tty| !tty.starts_with('?'));
    Some(PsRow {
        pid,
        up_secs,
        cpu,
        tty: tty.map(str::to_owned),
        args: fields.map(str::to_owned).collect(),
    })
}

// ps prints [[dd-]hh:]mm:ss.
fn parse_elapsed(text: &str) -> Option<u64> {
    let (days, clock) = match text.split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().ok()?, clock),
        None => (0, text),
    };
    let mut seconds = 0;
    for part in clock.split(':') {
        seconds = seconds * 60 + part.parse::<u64>().ok()?;
    }
    Some(days * 86_400 + seconds)
}

fn classify(row: PsRow) -> Option<Process> {
    let name = Path::new(row.args.first()?).file_name()?.to_str()?;
    let (kind, state_home, detail) = match (name, row.args.get(1).map(String::as_str)) {
        ("seer", None | Some("attach" | "peek" | "join")) => (
            Kind::Window,
            None,
            Detail::Window {
                terminal: row.tty.filter(|tty| Path::new("/dev").join(tty).exists()),
            },
        ),
        ("seer-runtime", Some(socket)) => {
            let socket = PathBuf::from(socket);
            (
                Kind::Runtime,
                socket.ancestors().nth(4).map(Path::to_path_buf),
                Detail::Runtime {
                    status: query_status(&socket),
                    socket,
                },
            )
        }
        ("seer-broker", Some(config)) => (
            Kind::Broker,
            broker_state_home(Path::new(config)),
            Detail::Broker,
        ),
        _ => return None,
    };
    Some(Process {
        pid: row.pid,
        kind,
        up_secs: row.up_secs,
        cpu: row.cpu,
        state_home,
        detail,
    })
}

fn query_status(socket: &Path) -> Option<RuntimeStatus> {
    let mut stream = crate::local::connect(socket, None).ok()?;
    stream.set_read_timeout(Some(STATUS_TIMEOUT)).ok()?;
    codec::encode(&mut stream, &ClientMsg::QueryStatus).ok()?;
    match codec::decode(&mut stream).ok()? {
        ServerMsg::Status {
            windows: Some(windows),
            shells: Some(shells),
            ..
        } => Some(RuntimeStatus::Counts { windows, shells }),
        ServerMsg::Status { .. } => Some(RuntimeStatus::Older),
        _ => None,
    }
}

fn broker_state_home(config: &Path) -> Option<PathBuf> {
    let table: toml::Table = toml::from_str(&fs::read_to_string(config).ok()?).ok()?;
    let state_dir = table.get("state_dir")?.as_str()?;
    Path::new(state_dir).parent().map(Path::to_path_buf)
}

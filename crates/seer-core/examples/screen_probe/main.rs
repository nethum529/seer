// Measurement probe for issue 417. It attaches one host window to a local
// runtime, joins the room as a guest that watches the host's first pane,
// types a script into the host window at a fixed rate, and records every
// screen update that arrives at the guest socket. The method is in
// docs/research/23-screen-data.md.
//
// Output, one line per screen update after the first key:
//   update,<label>,<cols>,<rows>,<seq>,<t_ms>,<bytes>,<frame_cols>,<frame_rows>,
//          <changed_cells>,<changed_rows>,<row_diff_bytes>,<cell_diff_bytes>,
//          <shift>,<scroll_cells>,<scroll_diff_bytes>
// and one closing line:
//   window,<label>,<cols>,<rows>,<keys>,<last_key_ms>,<window_ms>,<updates>,<bytes>
// The window runs from the first key to the last update. last_key_ms is
// when the last key was sent, so a run can be split into a typing part and
// an output part.
mod estimate;
mod link;
mod script;

use script::Step;
use seer_core::TerminalFrame;
use seer_core::proto::ServerMsg;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const SETTLE: Duration = Duration::from_millis(1500);
const READ_TIMEOUT: Duration = Duration::from_millis(100);

struct Options {
    socket: PathBuf,
    room: SocketAddr,
    host: String,
    viewer: String,
    viewer_credential: String,
    cols: u16,
    rows: u16,
    input: PathBuf,
    rate_ms: u64,
    quiet_ms: u64,
    hold_ms: u64,
    label: String,
}

impl Options {
    fn parse() -> io::Result<Self> {
        let mut options = Self {
            socket: PathBuf::new(),
            room: "127.0.0.1:0".parse().map_err(io::Error::other)?,
            host: "alice".into(),
            viewer: "bob".into(),
            viewer_credential: String::new(),
            cols: 80,
            rows: 24,
            input: PathBuf::new(),
            rate_ms: 100,
            quiet_ms: 2000,
            hold_ms: 90_000,
            label: "run".into(),
        };
        let mut arguments = std::env::args().skip(1);
        while let Some(name) = arguments.next() {
            let value = arguments
                .next()
                .ok_or_else(|| invalid(format!("{name} needs a value")))?;
            match name.as_str() {
                "--socket" => options.socket = value.into(),
                "--room" => options.room = value.parse().map_err(io::Error::other)?,
                "--host" => options.host = value,
                "--viewer" => options.viewer = value,
                "--viewer-credential" => options.viewer_credential = value,
                "--cols" => options.cols = number(&value)?,
                "--rows" => options.rows = number(&value)?,
                "--input" => options.input = value.into(),
                "--rate-ms" => options.rate_ms = number(&value)?,
                "--quiet-ms" => options.quiet_ms = number(&value)?,
                "--hold-ms" => options.hold_ms = number(&value)?,
                "--label" => options.label = value,
                other => return Err(invalid(format!("unknown option {other}"))),
            }
        }
        Ok(options)
    }
}

fn number<T: std::str::FromStr>(value: &str) -> io::Result<T> {
    value
        .parse()
        .map_err(|_| invalid(format!("not a number: {value}")))
}

fn invalid(reason: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, reason)
}

struct Update {
    seq: usize,
    at: Duration,
    bytes: usize,
    frame_cols: usize,
    frame_rows: usize,
    diff: estimate::Diff,
}

fn main() -> io::Result<()> {
    let options = Options::parse()?;
    let steps = script::parse(&std::fs::read_to_string(&options.input)?)?;
    let mut host = link::HostWindow::attach(&options.socket)?;
    host.resize(options.cols, options.rows)?;
    host.drain_in_background()?;
    let pane = host.pane.clone();
    let mut guest = link::Guest::join(options.room, &options.viewer, &options.viewer_credential)?;
    guest.wait_published(&options.host)?;
    guest.watch(&options.host, &pane, options.cols, options.rows)?;
    let mut previous = None;
    let settled = Instant::now() + SETTLE;
    guest.set_read_timeout(READ_TIMEOUT)?;
    while Instant::now() < settled {
        if let Ok((
            _,
            ServerMsg::Cells {
                frame, pane: shown, ..
            },
        )) = guest.read_frame()
            && shown == pane
        {
            previous = Some(frame);
        }
    }
    let done = Arc::new(AtomicBool::new(false));
    let last_key = Arc::new(AtomicU64::new(0));
    let key_count = script::key_count(&steps);
    let start = Instant::now();
    let feeder = spawn_feeder(
        host,
        steps,
        options.rate_ms,
        Arc::clone(&done),
        Arc::clone(&last_key),
        start,
    );
    let (updates, last) = capture(&mut guest, &options, &pane, previous, &done, start)?;
    feeder
        .join()
        .map_err(|_| io::Error::other("feeder panicked"))??;
    print_last_rows(last.as_ref());
    let summary = Summary {
        keys: key_count,
        last_key_ms: last_key.load(Ordering::SeqCst),
    };
    report(&options, &updates, &summary)
}

struct Summary {
    keys: usize,
    last_key_ms: u64,
}

// The final screen, so a person can check that each script left the shell at
// a prompt. Blank rows are skipped.
fn print_last_rows(frame: Option<&TerminalFrame>) {
    let Some(frame) = frame else {
        return;
    };
    for row in &frame.rows {
        let text: String = row.iter().map(|cell| cell.character).collect();
        if !text.trim().is_empty() {
            eprintln!("screen: {}", text.trim_end());
        }
    }
}

fn spawn_feeder(
    mut host: link::HostWindow,
    steps: Vec<Step>,
    rate_ms: u64,
    done: Arc<AtomicBool>,
    last_key: Arc<AtomicU64>,
    start: Instant,
) -> thread::JoinHandle<io::Result<()>> {
    thread::spawn(move || {
        let rate = Duration::from_millis(rate_ms);
        for step in steps {
            match step {
                Step::Event(event) => {
                    host.input(event)?;
                    last_key.store(millis(start.elapsed()), Ordering::SeqCst);
                    thread::sleep(rate);
                }
                Step::Wait(pause) => thread::sleep(pause),
            }
        }
        done.store(true, Ordering::SeqCst);
        Ok(())
    })
}

fn capture(
    guest: &mut link::Guest,
    options: &Options,
    pane: &str,
    mut previous: Option<TerminalFrame>,
    done: &AtomicBool,
    start: Instant,
) -> io::Result<(Vec<Update>, Option<TerminalFrame>)> {
    let quiet = Duration::from_millis(options.quiet_ms);
    let hold = Duration::from_millis(options.hold_ms);
    let mut updates = Vec::new();
    let mut last_update = start;
    loop {
        let now = Instant::now();
        if now - start > hold || (done.load(Ordering::SeqCst) && now - last_update > quiet) {
            return Ok((updates, previous));
        }
        let (bytes, message) = match guest.read_frame() {
            Ok(frame) => frame,
            Err(error) if is_timeout(&error) => continue,
            Err(error) => return Err(error),
        };
        let ServerMsg::Cells {
            frame, pane: shown, ..
        } = message
        else {
            continue;
        };
        if shown != pane {
            continue;
        }
        last_update = Instant::now();
        let diff = estimate::diff(previous.as_ref(), &frame, &options.host, pane)?;
        updates.push(Update {
            seq: updates.len() + 1,
            at: last_update - start,
            bytes,
            frame_cols: frame.rows.first().map_or(0, Vec::len),
            frame_rows: frame.rows.len(),
            diff,
        });
        previous = Some(frame);
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn report(options: &Options, updates: &[Update], summary: &Summary) -> io::Result<()> {
    let mut out = io::stdout().lock();
    let label = format!("{},{},{}", options.label, options.cols, options.rows);
    for update in updates {
        writeln!(
            out,
            "update,{label},{},{},{},{},{},{},{},{},{},{},{},{}",
            update.seq,
            update.at.as_millis(),
            update.bytes,
            update.frame_cols,
            update.frame_rows,
            update.diff.changed_cells,
            update.diff.changed_rows,
            update.diff.row_diff_bytes,
            update.diff.cell_diff_bytes,
            update.diff.shift,
            update.diff.scroll_cells,
            update.diff.scroll_diff_bytes,
        )?;
    }
    let window_ms = updates.last().map_or(0, |update| update.at.as_millis());
    let bytes: usize = updates.iter().map(|update| update.bytes).sum();
    writeln!(
        out,
        "window,{label},{},{},{window_ms},{},{bytes}",
        summary.keys,
        summary.last_key_ms,
        updates.len()
    )
}

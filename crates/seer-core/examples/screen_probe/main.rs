// Measurement probe for issues 417 and 411. It attaches one host window to
// a local runtime, joins the room as a guest that watches the host's first
// pane, types a script into the host window at a fixed rate, and records
// every screen update that arrives at the guest socket. For each update it
// also runs the scroll diff of seer_core::frame_diff against the previous
// frame, sizes it as a message, and checks that the diff applies back to
// the current frame. The method is in docs/research/23-screen-data.md.
//
// Output, one line per screen update after the first key:
//   update,<label>,<cols>,<rows>,<seq>,<t_ms>,<bytes>,<frame_cols>,<frame_rows>,
//          <changed_cells>,<shift>,<diff_cells>,<diff_bytes>,<sent_bytes>,
//          <kind>,<apply_ok>,<full_bytes>,<floor_ok>
// kind is full for a Cells and diff for a CellsDiff on the wire; bytes and
// sent_bytes are the wire bytes of that message. For a Cells the diff
// columns show what a diff would have cost. apply_ok is 1 when the diff
// applied gives the current frame. full_bytes is the size of a Cells with
// the current frame, and floor_ok is 1 when cells_floor is not above it.
// One closing line:
//   window,<label>,<cols>,<rows>,<keys>,<last_key_ms>,<window_ms>,<updates>,<bytes>,
//          <final_match>
// The window runs from the first key to the last update. last_key_ms is
// when the last key was sent, so a run can be split into a typing part and
// an output part. final_match is 1 when the whole screen that a Resync
// returns after the run equals the screen built from the diffs.
mod link;
mod measure;
mod script;

use script::Step;
use seer_core::TerminalFrame;
use seer_core::frame_diff::apply_next;
use seer_core::proto::ServerMsg;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const SETTLE: Duration = Duration::from_millis(1500);
const TRUTH_WAIT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_millis(100);

// The screen a viewer holds and its number.
type Held = (TerminalFrame, u64);

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
    measure: measure::Measure,
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
                frame,
                pane: shown,
                seq,
                ..
            },
        )) = guest.read_frame()
            && shown == pane
        {
            previous = Some((frame, seq));
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
    print_last_rows(last.as_ref().map(|(frame, _)| frame));
    let summary = Summary {
        keys: key_count,
        last_key_ms: last_key.load(Ordering::SeqCst),
        final_match: truth_check(&mut guest, &options.host, &pane, last)?,
    };
    report(&options, &updates, &summary)
}

struct Summary {
    keys: usize,
    last_key_ms: u64,
    final_match: bool,
}

// The screen built from the diffs must equal the whole screen the runtime
// sends for a Resync. A diff still in flight is applied first.
fn truth_check(
    guest: &mut link::Guest,
    host: &str,
    pane: &str,
    mut held: Option<Held>,
) -> io::Result<bool> {
    guest.resync(host, pane)?;
    let deadline = Instant::now() + TRUTH_WAIT;
    while Instant::now() < deadline {
        let message = match guest.read_frame() {
            Ok((_, message)) => message,
            Err(error) if is_timeout(&error) => continue,
            Err(error) => return Err(error),
        };
        match message {
            ServerMsg::Cells {
                frame, pane: shown, ..
            } if shown == pane => return Ok(held.map(|(held, _)| held).as_ref() == Some(&frame)),
            ServerMsg::CellsDiff {
                pane: shown,
                seq,
                diff,
                ..
            } if shown == pane => {
                held = held
                    .and_then(|(held, held_seq)| apply_next(&held, held_seq, seq, &diff))
                    .map(|frame| (frame, seq));
            }
            _ => {}
        }
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "the whole screen never came after the run",
    ))
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
    mut previous: Option<Held>,
    done: &AtomicBool,
    start: Instant,
) -> io::Result<(Vec<Update>, Option<Held>)> {
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
        let header = |seq| measure::Header {
            user: &options.host,
            pane,
            seq,
        };
        let (measure, next) = match message {
            ServerMsg::Cells {
                frame,
                pane: shown,
                seq,
                ..
            } if shown == pane => (
                measure::whole(held_frame(previous.as_ref()), &frame, &header(seq), bytes)?,
                Some((frame, seq)),
            ),
            ServerMsg::CellsDiff {
                pane: shown,
                seq,
                diff,
                ..
            } if shown == pane => {
                let held = previous
                    .as_ref()
                    .map(|(frame, held_seq)| (frame, *held_seq));
                let (measure, frame) = measure::applied(held, &diff, &header(seq), bytes)?;
                (measure, frame.map(|frame| (frame, seq)))
            }
            _ => continue,
        };
        last_update = Instant::now();
        previous = next.or(previous);
        let held = held_frame(previous.as_ref());
        updates.push(Update {
            seq: updates.len() + 1,
            at: last_update - start,
            bytes,
            frame_cols: held
                .and_then(|frame| frame.rows.first())
                .map_or(0, Vec::len),
            frame_rows: held.map_or(0, |frame| frame.rows.len()),
            measure,
        });
    }
}

fn held_frame(held: Option<&Held>) -> Option<&TerminalFrame> {
    held.map(|(frame, _)| frame)
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
            "update,{label},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            update.seq,
            update.at.as_millis(),
            update.bytes,
            update.frame_cols,
            update.frame_rows,
            update.measure.changed_cells,
            update.measure.shift,
            update.measure.diff_cells,
            update.measure.diff_bytes,
            update.measure.sent_bytes,
            if update.measure.full { "full" } else { "diff" },
            u8::from(update.measure.apply_ok),
            update.measure.full_bytes,
            u8::from(update.measure.floor_ok),
        )?;
    }
    let window_ms = updates.last().map_or(0, |update| update.at.as_millis());
    let bytes: usize = updates.iter().map(|update| update.bytes).sum();
    writeln!(
        out,
        "window,{label},{},{},{window_ms},{},{bytes},{}",
        summary.keys,
        summary.last_key_ms,
        updates.len(),
        u8::from(summary.final_match)
    )
}

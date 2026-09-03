use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::VecDeque;
use std::fmt::Display;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

const OUTPUT_LIMIT: usize = 1024 * 1024;
const READ_BUFFER_SIZE: usize = 8 * 1024;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output: Arc<Mutex<OutputBuffers>>,
}

#[derive(Default)]
struct OutputBuffers {
    snapshot: VecDeque<u8>,
    pending: VecDeque<u8>,
}

impl PtySession {
    pub fn start(command: CommandBuilder, cols: u16, rows: u16) -> io::Result<Self> {
        let pair = native_pty_system()
            .openpty(pty_size(cols, rows))
            .map_err(to_io_error)?;
        let reader = pair.master.try_clone_reader().map_err(to_io_error)?;
        let writer = pair.master.take_writer().map_err(to_io_error)?;
        let output = Arc::new(Mutex::new(OutputBuffers::default()));
        let reader_output = Arc::clone(&output);

        let _reader_task = thread::Builder::new()
            .name("pty-output-reader".to_owned())
            .spawn(move || read_output(reader, &reader_output))?;
        let child = pair.slave.spawn_command(command).map_err(to_io_error)?;

        Ok(Self {
            master: pair.master,
            child,
            writer,
            output,
        })
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(bytes)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        self.master
            .resize(pty_size(cols, rows))
            .map_err(to_io_error)
    }

    pub fn is_alive(&mut self) -> io::Result<bool> {
        self.child.try_wait().map(|status| status.is_none())
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    pub fn snapshot(&self) -> Vec<u8> {
        let output = lock_mutex(&self.output);
        output.snapshot.iter().copied().collect()
    }

    pub fn drain_output(&self) -> Vec<u8> {
        let mut output = lock_mutex(&self.output);
        output.pending.drain(..).collect()
    }
}

fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn read_output(mut reader: Box<dyn Read + Send>, output: &Mutex<OutputBuffers>) {
    let mut bytes = [0; READ_BUFFER_SIZE];

    loop {
        match reader.read(&mut bytes) {
            Ok(0) => return,
            Ok(count) => append_output(output, &bytes[..count]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

fn append_output(output: &Mutex<OutputBuffers>, bytes: &[u8]) {
    let mut output = lock_mutex(output);
    append_bounded(&mut output.snapshot, bytes);
    append_bounded(&mut output.pending, bytes);
}

// Discard the oldest bytes so detached output stays bounded and keeps the latest state.
fn append_bounded(output: &mut VecDeque<u8>, bytes: &[u8]) {
    output.extend(bytes);
    let excess = output.len().saturating_sub(OUTPUT_LIMIT);
    output.drain(..excess);
}

pub(crate) fn lock_mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(value) => value,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn to_io_error(error: impl Display) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
    const POLL_INTERVAL: Duration = Duration::from_millis(10);

    #[test]
    fn session_captures_output_accepts_input_resizes_and_stops() {
        let mut command = CommandBuilder::new("sh");
        command.args(["-c", "echo hello; cat"]);
        let mut session = PtySession::start(command, 80, 24).expect("PTY must start");

        assert!(wait_for_output(&session, b"hello"));
        assert!(
            session
                .drain_output()
                .windows(5)
                .any(|bytes| bytes == b"hello")
        );
        assert!(session.drain_output().is_empty());
        assert!(session.snapshot().windows(5).any(|bytes| bytes == b"hello"));
        session
            .write_input(b"input text\n")
            .expect("input write must succeed");
        assert!(wait_for_output(&session, b"input text"));
        session.resize(100, 40).expect("resize must succeed");
        assert!(session.is_alive().expect("process state must be readable"));

        session.kill().expect("kill must succeed");
        assert!(wait_for_exit(&mut session));
    }

    #[test]
    fn output_buffers_keep_last_mebibyte_while_not_drained() {
        let output = Mutex::new(OutputBuffers::default());
        let bytes: Vec<u8> = (0..=OUTPUT_LIMIT)
            .map(|index| (index % usize::from(u8::MAX)) as u8)
            .collect();
        append_output(&output, &bytes);

        let output = output.into_inner().expect("output lock must be valid");
        assert_eq!(output.snapshot.len(), OUTPUT_LIMIT);
        assert!(
            output
                .snapshot
                .iter()
                .copied()
                .eq(bytes[1..].iter().copied())
        );
        assert_eq!(output.pending.len(), OUTPUT_LIMIT);
        assert!(
            output
                .pending
                .iter()
                .copied()
                .eq(bytes[1..].iter().copied())
        );
    }

    fn wait_for_output(session: &PtySession, expected: &[u8]) -> bool {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while Instant::now() < deadline {
            if session
                .snapshot()
                .windows(expected.len())
                .any(|window| window == expected)
            {
                return true;
            }
            thread::sleep(POLL_INTERVAL);
        }
        false
    }

    fn wait_for_exit(session: &mut PtySession) -> bool {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while Instant::now() < deadline {
            match session.is_alive() {
                Ok(false) => return true,
                Ok(true) => thread::sleep(POLL_INTERVAL),
                Err(_) => return false,
            }
        }
        false
    }
}

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::VecDeque;
use std::fmt::Display;
use std::fs;
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

    pub fn has_exited(&mut self) -> io::Result<bool> {
        self.child.try_wait().map(|status| status.is_some())
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    pub fn foreground_name(&self) -> String {
        let Some(descriptor) = self.master.as_raw_fd() else {
            return String::new();
        };
        // SAFETY: the descriptor belongs to this session master PTY and stays open for the call.
        let group = unsafe { libc::tcgetpgrp(descriptor) };
        if group <= 0 {
            return String::new();
        }
        process_name(group)
    }

    pub fn drain_output(&self) -> Vec<u8> {
        let mut output = lock_mutex(&self.output);
        output.pending.drain(..).collect()
    }
}

#[cfg(target_os = "linux")]
fn process_name(group: i32) -> String {
    fs::read_to_string(format!("/proc/{group}/comm"))
        .map(|name| name.trim().to_owned())
        .unwrap_or_default()
}

#[cfg(not(target_os = "linux"))]
fn process_name(_group: i32) -> String {
    String::new()
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

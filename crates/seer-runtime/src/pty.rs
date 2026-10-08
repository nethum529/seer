use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::fmt::Display;
use std::io::{self, Read, Write};
use std::os::fd::RawFd;
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

// Alacritty reads the PTY in blocks of this size.
const READ_BUFFER_SIZE: usize = 64 * 1024;

pub(crate) type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Writer,
}

impl PtySession {
    /// Starts the command on a new PTY. The reader thread gives each block
    /// of output and the PTY writer to `on_output`, so a query gets its
    /// answer without a wait for a poll.
    pub(crate) fn start(
        command: CommandBuilder,
        cols: u16,
        rows: u16,
        on_output: impl FnMut(&[u8], &Writer) + Send + 'static,
    ) -> io::Result<Self> {
        let pair = native_pty_system()
            .openpty(pty_size(cols, rows))
            .map_err(to_io_error)?;
        let reader = pair.master.try_clone_reader().map_err(to_io_error)?;
        let writer: Writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(to_io_error)?));
        let replies = Arc::clone(&writer);
        let ready = Readiness::new(pair.master.as_raw_fd())?;

        let _reader_task = thread::Builder::new()
            .name("pty-output-reader".to_owned())
            .spawn(move || read_output(reader, &ready, &replies, on_output))?;
        let child = pair.slave.spawn_command(command).map_err(to_io_error)?;

        Ok(Self {
            master: pair.master,
            child,
            writer,
        })
    }

    pub(crate) fn writer(&self) -> Writer {
        Arc::clone(&self.writer)
    }

    pub fn write_input(&self, bytes: &[u8]) -> io::Result<()> {
        lock_mutex(&self.writer).write_all(bytes)
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
}

#[cfg(target_os = "linux")]
fn process_name(group: i32) -> String {
    std::fs::read_to_string(format!("/proc/{group}/comm"))
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

fn read_output(
    mut reader: Box<dyn Read + Send>,
    ready: &Readiness,
    writer: &Writer,
    mut on_output: impl FnMut(&[u8], &Writer),
) {
    let mut bytes = vec![0; READ_BUFFER_SIZE];

    loop {
        ready.wait();
        match reader.read(&mut bytes) {
            Ok(0) => return,
            Ok(count) => on_output(&bytes[..count], writer),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

// Linux tty quirk: while an epoll set holds the master descriptor, the
// shell side writes about 20 percent faster (cat of plain text: 136 ms
// against 163 ms). A poll(2) wait or an epoll set on a dup of the master
// does not give the gain. Alacritty waits the same way.
// docs/research/25-terminal-speed.md has the measurements.
#[cfg(target_os = "linux")]
struct Readiness(Option<OwnedFd>);

#[cfg(target_os = "linux")]
impl Readiness {
    fn new(master: Option<RawFd>) -> io::Result<Self> {
        let Some(master) = master else {
            return Ok(Self(None));
        };
        // SAFETY: epoll_create1 takes no pointers.
        let descriptor = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: epoll_create1 returned a new descriptor that nothing else owns.
        let epoll = unsafe { OwnedFd::from_raw_fd(descriptor) };
        let mut event = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: 0,
        };
        // SAFETY: both descriptors are open and event lives for the call.
        if unsafe { libc::epoll_ctl(descriptor, libc::EPOLL_CTL_ADD, master, &mut event) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(Some(epoll)))
    }

    fn wait(&self) {
        let Some(epoll) = &self.0 else {
            return;
        };
        let mut event = libc::epoll_event { events: 0, u64: 0 };
        // SAFETY: the epoll descriptor is open and event has room for the one entry asked for.
        unsafe { libc::epoll_wait(epoll.as_raw_fd(), &mut event, 1, -1) };
    }
}

#[cfg(not(target_os = "linux"))]
struct Readiness;

#[cfg(not(target_os = "linux"))]
impl Readiness {
    fn new(_master: Option<RawFd>) -> io::Result<Self> {
        Ok(Self)
    }

    fn wait(&self) {}
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

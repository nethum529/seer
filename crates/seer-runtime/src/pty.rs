use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::fmt::Display;
use std::io::{self, Read, Write};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

// Alacritty reads the PTY in blocks of this size.
const READ_BUFFER_SIZE: usize = 64 * 1024;
// The same bound as the raw output queued for a window (outbox.rs).
const ANSWER_LIMIT: usize = 64 * 1024;

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

/// The queue of answers to queries for the answer thread of a PTY.
#[derive(Clone)]
pub(crate) struct Answers {
    queue: Sender<Vec<u8>>,
    pending: Arc<AtomicUsize>,
}

impl Answers {
    /// Queues `bytes`, or drops them when ANSWER_LIMIT bytes wait. Only a
    /// program that does not read its input fills the queue. To wait for
    /// room would stop the PTY reader, and then that program would never
    /// read.
    pub(crate) fn send(&self, bytes: Vec<u8>) {
        let count = bytes.len();
        if self.pending.fetch_add(count, Ordering::Relaxed) + count > ANSWER_LIMIT
            || self.queue.send(bytes).is_err()
        {
            self.pending.fetch_sub(count, Ordering::Relaxed);
        }
    }
}

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Writer,
    answers: Answers,
}

impl PtySession {
    /// Starts the command on a new PTY. The reader thread gives each block
    /// of output to `on_output`, so a query gets its answer without a wait
    /// for a poll. An answer thread writes the answers sent to `Answers` in
    /// order, so the reader does not wait while input waits for the program.
    pub(crate) fn start(
        command: CommandBuilder,
        cols: u16,
        rows: u16,
        on_output: impl FnMut(&[u8], &Answers) + Send + 'static,
    ) -> io::Result<Self> {
        let pair = native_pty_system()
            .openpty(pty_size(cols, rows))
            .map_err(to_io_error)?;
        let reader = pair.master.try_clone_reader().map_err(to_io_error)?;
        let writer: Writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(to_io_error)?));
        let (queue, waiting) = mpsc::channel();
        let answers = Answers {
            queue,
            pending: Arc::new(AtomicUsize::new(0)),
        };
        let pending = Arc::clone(&answers.pending);
        let replies = Arc::clone(&writer);
        let ready = Readiness::new(pair.master.as_raw_fd())?;

        let _answer_task = thread::Builder::new()
            .name("pty-answer-writer".to_owned())
            .spawn(move || write_answers(waiting, &pending, &replies))?;
        let reader_answers = answers.clone();
        let _reader_task = thread::Builder::new()
            .name("pty-output-reader".to_owned())
            .spawn(move || read_output(reader, &ready, &reader_answers, on_output))?;
        let child = pair.slave.spawn_command(command).map_err(to_io_error)?;

        Ok(Self {
            master: pair.master,
            child,
            writer,
            answers,
        })
    }

    pub(crate) fn answers(&self) -> Answers {
        self.answers.clone()
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

    pub(crate) fn observation_fd(&self) -> io::Result<Option<OwnedFd>> {
        let Some(descriptor) = self.master.as_raw_fd() else {
            return Ok(None);
        };
        // SAFETY: descriptor is open; fcntl duplicates it without using pointers.
        let duplicate = unsafe { libc::fcntl(descriptor, libc::F_DUPFD_CLOEXEC, 0) };
        if duplicate < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fcntl returned a new descriptor that no other object owns.
        Ok(Some(unsafe { OwnedFd::from_raw_fd(duplicate) }))
    }

    pub(crate) fn process_id(&self) -> Option<u32> {
        self.child.process_id()
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

fn read_output(
    mut reader: Box<dyn Read + Send>,
    ready: &Readiness,
    answers: &Answers,
    mut on_output: impl FnMut(&[u8], &Answers),
) {
    let mut bytes = vec![0; READ_BUFFER_SIZE];

    loop {
        ready.wait();
        match reader.read(&mut bytes) {
            Ok(0) => return,
            Ok(count) => on_output(&bytes[..count], answers),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

fn write_answers(waiting: Receiver<Vec<u8>>, pending: &AtomicUsize, writer: &Writer) {
    for bytes in waiting {
        if let Err(error) = lock_mutex(writer).write_all(&bytes) {
            eprintln!("pane reply write failed: {error}");
        }
        pending.fetch_sub(bytes.len(), Ordering::Relaxed);
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

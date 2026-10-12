use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use futures::channel::mpsc::unbounded;
use seer::ServerEntry;
use seer_core::frame_diff::FrameDiff;
use seer_core::proto::codec::{self, Incoming, RawEvent};
use seer_core::proto::{ClientMsg, ServerMsg, TerminalInfo};
use seer_core::{
    Cell, Cursor, InputEvent, TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalFrame,
    TerminalInput, Tree,
};
use seer_net::Socket;
use seer_runtime::PaneGrid;

use crate::room::{Room, transport::RoomEvent};

pub(crate) mod frames;

pub(crate) enum Event {
    Message(ServerMsg),
    Screen,
    Room(RoomEvent),
}

pub(crate) type Messages = Pin<Box<dyn futures::Stream<Item = io::Result<Event>> + Send>>;
pub(crate) type Terminal = Arc<Mutex<Option<Shown>>>;
type Writer = Arc<Mutex<Socket>>;

/// The window asks for the raw output. A runtime from before raw output
/// ignores the ask and sends cell frames, which the window shows as is.
pub(crate) enum Shown {
    Raw(Box<PaneGrid>),
    Frame {
        frame: TerminalFrame,
        seq: u64,
        fresh: bool,
    },
}

impl Shown {
    pub(crate) fn rows(&self) -> usize {
        match self {
            Self::Raw(grid) => usize::from(grid.size().1),
            Self::Frame { frame, .. } => frame.rows.len(),
        }
    }

    /// The rows that changed since the last call. None means every row.
    pub(crate) fn take_damage(&mut self) -> Option<Vec<usize>> {
        match self {
            Self::Raw(grid) => grid.take_damage(),
            Self::Frame { fresh, .. } => (!std::mem::take(fresh)).then(Vec::new),
        }
    }

    pub(crate) fn row(&self, index: usize) -> Vec<Cell> {
        match self {
            Self::Raw(grid) => grid.row(index),
            Self::Frame { frame, .. } => frame.rows.get(index).cloned().unwrap_or_default(),
        }
    }

    pub(crate) fn cursor(&self) -> Cursor {
        match self {
            Self::Raw(grid) => grid.cursor(),
            Self::Frame { frame, .. } => frame.cursor,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Target {
    workspace: String,
    tab: String,
    pane: String,
}

pub(crate) struct Link {
    writer: Writer,
    user: String,
    target: Option<Target>,
    tree: Tree,
    terminals: Vec<TerminalInfo>,
    size: Option<(u16, u16)>,
    terminal: Terminal,
    woken: Arc<AtomicBool>,
    resyncing: bool,
    room: String,
    pub(crate) people: Room,
}

impl Link {
    pub(crate) fn open(
        socket: Socket,
        tree: &Tree,
        user: String,
        room: String,
    ) -> io::Result<(Self, Messages)> {
        let reader = socket.clone();
        let mut link = Self {
            writer: Arc::new(Mutex::new(socket)),
            people: Room::new(user.clone()),
            user,
            target: first_target(tree),
            tree: tree.clone(),
            terminals: Vec::new(),
            size: None,
            terminal: Terminal::default(),
            woken: Arc::default(),
            resyncing: false,
            room,
        };
        link.send(&ClientMsg::TerminalCapabilities {
            capabilities: TerminalCapabilities {
                protocol_version: TERMINAL_PROTOCOL_VERSION,
                raw_output: true,
            },
        })?;
        let messages = read_in_background(
            reader,
            Arc::clone(&link.writer),
            Arc::clone(&link.terminal),
            Arc::clone(&link.woken),
        );
        Ok((link, messages))
    }

    pub(crate) fn terminal(&self) -> &Terminal {
        if self.readonly() {
            &self.people.terminal
        } else {
            &self.terminal
        }
    }

    pub(crate) fn join_room(&mut self, server: ServerEntry, person: Option<String>) -> Messages {
        self.people.join(server, person)
    }

    pub(crate) fn readonly(&self) -> bool {
        self.people.readonly()
    }

    pub(crate) fn select_person(&mut self, user: &str) -> io::Result<()> {
        self.people.select_person(user);
        if let Some((cols, rows)) = self.size {
            self.people.resize(cols, rows);
        }
        if !self.readonly() {
            self.watch()?;
        }
        Ok(())
    }

    pub(crate) fn receive(&mut self, event: Event) -> io::Result<()> {
        match event {
            Event::Room(event) => {
                let remote = self.readonly();
                self.people.receive(event);
                if remote && !self.readonly() {
                    self.watch()?;
                }
                Ok(())
            }
            Event::Message(ServerMsg::Tree { tree }) => {
                self.retarget(&tree)?;
                self.tree = tree;
                Ok(())
            }
            Event::Message(ServerMsg::Terminals { user, terminals }) if user == self.user => {
                self.terminals = terminals;
                Ok(())
            }
            Event::Message(ServerMsg::Cells {
                user,
                pane,
                frame,
                seq,
            }) if self.shows(&user, &pane) => {
                frames::replace(&self.terminal, frame, seq);
                self.resyncing = false;
                Ok(())
            }
            Event::Message(ServerMsg::CellsDiff {
                user,
                pane,
                seq,
                diff,
            }) if self.shows(&user, &pane) && !self.resyncing => {
                self.apply_diff(user, pane, seq, &diff)
            }
            Event::Message(_) => Ok(()),
            Event::Screen => {
                self.woken.store(false, Ordering::Release);
                Ok(())
            }
        }
    }

    pub(crate) fn terminals(&self) -> &[TerminalInfo] {
        if self.readonly() {
            self.people.terminals()
        } else {
            &self.terminals
        }
    }

    pub(crate) fn room(&self) -> &str {
        &self.room
    }

    pub(crate) fn rename(&mut self, pane: &str, title: String) -> io::Result<()> {
        if self.readonly() {
            return Ok(());
        }
        let Some(target) = pane_target(&self.tree, pane) else {
            return Ok(());
        };
        self.send(&ClientMsg::RenameTab {
            workspace: target.workspace,
            tab: target.tab,
            title,
        })
    }

    pub(crate) fn new_terminal(&mut self) -> io::Result<()> {
        if self.readonly() {
            return Ok(());
        }
        let workspace = self
            .target
            .as_ref()
            .map(|target| target.workspace.clone())
            .or_else(|| {
                self.tree
                    .workspaces
                    .first()
                    .map(|workspace| workspace.id.clone())
            });
        if let Some(workspace) = workspace {
            self.send(&ClientMsg::CreateTab { workspace })?;
        }
        Ok(())
    }

    pub(crate) fn shown(&self) -> Option<&str> {
        if self.readonly() {
            self.people.pane.as_deref()
        } else {
            self.target.as_ref().map(|target| target.pane.as_str())
        }
    }

    pub(crate) fn input(&mut self, event: InputEvent) -> io::Result<()> {
        if self.readonly() {
            return Ok(());
        }
        let Some(target) = self.target.clone() else {
            return Ok(());
        };
        self.send(&ClientMsg::TerminalInput {
            workspace: target.workspace,
            tab: target.tab,
            pane: target.pane,
            input: TerminalInput::new(event),
        })
    }

    pub(crate) fn select(&mut self, pane: &str) -> io::Result<()> {
        if self.readonly() {
            self.people.select_pane(pane);
            return Ok(());
        }
        let Some(target) = pane_target(&self.tree, pane) else {
            return Ok(());
        };
        if self.target.as_ref() == Some(&target) {
            return Ok(());
        }
        self.show(Some(target))
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {
        if cols == 0 || rows == 0 || self.size == Some((cols, rows)) {
            return Ok(());
        }
        self.size = Some((cols, rows));
        self.people.resize(cols, rows);
        if self.readonly() {
            Ok(())
        } else {
            self.watch()
        }
    }

    fn apply_diff(
        &mut self,
        user: String,
        pane: String,
        seq: u64,
        diff: &FrameDiff,
    ) -> io::Result<()> {
        if frames::apply(&self.terminal, seq, diff) {
            return Ok(());
        }
        self.resyncing = true;
        self.send(&ClientMsg::Resync { user, pane })
    }

    fn retarget(&mut self, tree: &Tree) -> io::Result<()> {
        let still_there = self.target.as_ref().is_some_and(|target| {
            tree.workspaces.iter().any(|workspace| {
                workspace.id == target.workspace
                    && workspace.tabs.iter().any(|tab| {
                        tab.id == target.tab && tab.panes.iter().any(|pane| pane.id == target.pane)
                    })
            })
        });
        if still_there {
            return Ok(());
        }
        self.show(first_target(tree))
    }

    fn show(&mut self, target: Option<Target>) -> io::Result<()> {
        self.target = target;
        *lock(&self.terminal) = None;
        self.resyncing = false;
        self.watch()
    }

    // The runtime gives the size to the window that claimed it last, so the
    // TUI and this window can show the same terminal.
    fn watch(&mut self) -> io::Result<()> {
        let (Some(target), Some((cols, rows))) = (self.target.clone(), self.size) else {
            return Ok(());
        };
        self.send(&ClientMsg::Watch {
            user: self.user.clone(),
            pane: target.pane,
            cols,
            rows,
            viewer: true,
        })?;
        self.send(&ClientMsg::Resize {
            workspace: target.workspace,
            tab: target.tab,
            cols,
            rows,
        })
    }

    fn shows(&self, user: &str, pane: &str) -> bool {
        user == self.user
            && self
                .target
                .as_ref()
                .is_some_and(|target| target.pane == pane)
    }

    fn send(&mut self, message: &ClientMsg) -> io::Result<()> {
        send(&self.writer, message)
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn send(writer: &Writer, message: &ClientMsg) -> io::Result<()> {
    codec::encode(&mut *lock(writer), message)
}

fn first_target(tree: &Tree) -> Option<Target> {
    let workspace = tree.workspaces.first()?;
    let tab = workspace.tabs.first()?;
    let pane = tab
        .layout
        .focused
        .clone()
        .or_else(|| tab.panes.first().map(|pane| pane.id.clone()))?;
    Some(Target {
        workspace: workspace.id.clone(),
        tab: tab.id.clone(),
        pane,
    })
}

fn pane_target(tree: &Tree, pane: &str) -> Option<Target> {
    tree.workspaces.iter().find_map(|workspace| {
        workspace
            .tabs
            .iter()
            .find(|tab| tab.panes.iter().any(|candidate| candidate.id == pane))
            .map(|tab| Target {
                workspace: workspace.id.clone(),
                tab: tab.id.clone(),
                pane: pane.to_owned(),
            })
    })
}

// The raw output is parsed here, before the next read, so the runtime
// cannot run far ahead of the screen. A mark comes after the output that
// leads to a query, and the runtime holds the answer until this thread
// confirms the mark. The main thread gets one wake for all the output
// that comes before it draws.
fn read_in_background(
    mut socket: Socket,
    writer: Writer,
    terminal: Terminal,
    woken: Arc<AtomicBool>,
) -> Messages {
    let (sender, receiver) = unbounded();
    thread::spawn(move || {
        loop {
            let event = match codec::decode_incoming(&mut socket) {
                Ok(Incoming::Message(message)) => Ok(Event::Message(message)),
                Ok(Incoming::Raw(RawEvent::Mark(mark))) => {
                    match send(&writer, &ClientMsg::Parsed { mark }) {
                        Ok(()) => continue,
                        Err(error) => Err(error),
                    }
                }
                Ok(Incoming::Raw(raw)) => {
                    apply(&terminal, raw);
                    if woken.swap(true, Ordering::AcqRel) {
                        continue;
                    }
                    Ok(Event::Screen)
                }
                Err(error) => Err(error),
            };
            let failed = event.is_err();
            if sender.unbounded_send(event).is_err() || failed {
                break;
            }
        }
    });
    Box::pin(receiver)
}

// The runtime ends a synchronized update by time and sends SyncEnd. The
// mirror does not check the time itself, so both end it at the same byte.
fn apply(terminal: &Terminal, event: RawEvent) {
    let mut shown = lock(terminal);
    match (event, &mut *shown) {
        (RawEvent::Start { cols, rows }, shown) => {
            *shown = Some(Shown::Raw(Box::new(PaneGrid::mirror(cols, rows))));
        }
        (RawEvent::Output(bytes), Some(Shown::Raw(grid))) => {
            grid.feed(&bytes);
        }
        (RawEvent::Size { cols, rows }, Some(Shown::Raw(grid))) => grid.resize(cols, rows),
        (RawEvent::SyncEnd, Some(Shown::Raw(grid))) => {
            grid.end_sync();
        }
        _ => {}
    }
}

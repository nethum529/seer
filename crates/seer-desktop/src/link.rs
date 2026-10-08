use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use seer_core::proto::codec::{self, Incoming, RawEvent};
use seer_core::proto::{ClientMsg, ServerMsg, TerminalInfo};
use seer_core::{TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, Tree};
use seer_net::Socket;
use seer_runtime::PaneGrid;

pub(crate) enum Event {
    Message(ServerMsg),
    Screen,
}

pub(crate) type Messages = UnboundedReceiver<io::Result<Event>>;
pub(crate) type Terminal = Arc<Mutex<Option<PaneGrid>>>;

#[derive(Clone, PartialEq, Eq)]
struct Target {
    workspace: String,
    tab: String,
    pane: String,
}

pub(crate) struct Link {
    socket: Socket,
    user: String,
    target: Option<Target>,
    tree: Tree,
    terminals: Vec<TerminalInfo>,
    size: Option<(u16, u16)>,
    terminal: Terminal,
    woken: Arc<AtomicBool>,
}

impl Link {
    pub(crate) fn open(socket: Socket, tree: &Tree, user: String) -> io::Result<(Self, Messages)> {
        let mut link = Self {
            socket,
            user,
            target: first_target(tree),
            tree: tree.clone(),
            terminals: Vec::new(),
            size: None,
            terminal: Terminal::default(),
            woken: Arc::default(),
        };
        link.send(&ClientMsg::TerminalCapabilities {
            capabilities: TerminalCapabilities {
                protocol_version: TERMINAL_PROTOCOL_VERSION,
                raw_output: true,
            },
        })?;
        let messages = read_in_background(
            link.socket.clone(),
            Arc::clone(&link.terminal),
            Arc::clone(&link.woken),
        );
        Ok((link, messages))
    }

    pub(crate) fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    pub(crate) fn receive(&mut self, event: Event) -> io::Result<()> {
        match event {
            Event::Message(ServerMsg::Tree { tree }) => {
                self.retarget(&tree)?;
                self.tree = tree;
                Ok(())
            }
            Event::Message(ServerMsg::Terminals { user, terminals }) if user == self.user => {
                self.terminals = terminals;
                Ok(())
            }
            Event::Message(_) => Ok(()),
            Event::Screen => {
                self.woken.store(false, Ordering::Release);
                Ok(())
            }
        }
    }

    pub(crate) fn terminals(&self) -> &[TerminalInfo] {
        &self.terminals
    }

    pub(crate) fn shown(&self) -> Option<&str> {
        self.target.as_ref().map(|target| target.pane.as_str())
    }

    pub(crate) fn select(&mut self, pane: &str) -> io::Result<()> {
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
        self.watch()
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
        *self.terminal.lock().unwrap_or_else(PoisonError::into_inner) = None;
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

    fn send(&mut self, message: &ClientMsg) -> io::Result<()> {
        codec::encode(&mut self.socket, message)
    }
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
// cannot run far ahead of the screen. The main thread gets one wake for
// all the output that comes before it draws.
fn read_in_background(mut socket: Socket, terminal: Terminal, woken: Arc<AtomicBool>) -> Messages {
    let (sender, receiver) = unbounded();
    thread::spawn(move || {
        loop {
            let event = match codec::decode_incoming(&mut socket) {
                Ok(Incoming::Message(message)) => Ok(Event::Message(message)),
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
    receiver
}

// The runtime ends a synchronized update by time and sends SyncEnd. The
// mirror does not check the time itself, so both end it at the same byte.
fn apply(terminal: &Terminal, event: RawEvent) {
    let mut held = terminal.lock().unwrap_or_else(PoisonError::into_inner);
    match (event, &mut *held) {
        (RawEvent::Start { cols, rows }, held) => *held = Some(PaneGrid::mirror(cols, rows)),
        (RawEvent::Output(bytes), Some(grid)) => {
            grid.feed(&bytes);
        }
        (RawEvent::Size { cols, rows }, Some(grid)) => grid.resize(cols, rows),
        (RawEvent::SyncEnd, Some(grid)) => {
            grid.end_sync();
        }
        (_, None) => {}
    }
}

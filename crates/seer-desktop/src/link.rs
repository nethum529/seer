use std::io;
use std::thread;

use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use seer_core::frame_diff::apply_next;
use seer_core::proto::{ClientMsg, ServerMsg, TerminalInfo, codec};
use seer_core::{TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalFrame, Tree};
use seer_net::Socket;

pub(crate) type Messages = UnboundedReceiver<io::Result<ServerMsg>>;

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
    screen: Option<(TerminalFrame, u64)>,
    resyncing: bool,
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
            screen: None,
            resyncing: false,
        };
        link.send(&ClientMsg::TerminalCapabilities {
            capabilities: TerminalCapabilities {
                protocol_version: TERMINAL_PROTOCOL_VERSION,
            },
        })?;
        let messages = read_in_background(link.socket.clone());
        Ok((link, messages))
    }

    pub(crate) fn frame(&self) -> Option<&TerminalFrame> {
        self.screen.as_ref().map(|(frame, _)| frame)
    }

    pub(crate) fn receive(&mut self, message: ServerMsg) -> io::Result<()> {
        match message {
            ServerMsg::Tree { tree } => {
                self.retarget(&tree)?;
                self.tree = tree;
                Ok(())
            }
            ServerMsg::Terminals { user, terminals } if user == self.user => {
                self.terminals = terminals;
                Ok(())
            }
            ServerMsg::Cells {
                user,
                pane,
                frame,
                seq,
            } if self.shows(&user, &pane) => {
                self.screen = Some((frame, seq));
                self.resyncing = false;
                Ok(())
            }
            ServerMsg::CellsDiff {
                user,
                pane,
                seq,
                diff,
            } if self.shows(&user, &pane) && !self.resyncing => {
                self.screen = self
                    .screen
                    .take()
                    .and_then(|(held, held_seq)| apply_next(&held, held_seq, seq, &diff))
                    .map(|frame| (frame, seq));
                if self.screen.is_some() {
                    return Ok(());
                }
                // One request per gap. Later diffs wait for the whole screen.
                self.resyncing = true;
                self.send(&ClientMsg::Resync { user, pane })
            }
            _ => Ok(()),
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
        self.screen = None;
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

fn read_in_background(mut socket: Socket) -> Messages {
    let (sender, receiver) = unbounded();
    thread::spawn(move || {
        loop {
            let message = codec::decode(&mut socket);
            let failed = message.is_err();
            if sender.unbounded_send(message).is_err() || failed {
                break;
            }
        }
    });
    receiver
}

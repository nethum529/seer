use seer_core::PaneSize;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc::{self, SyncSender};
use std::time::Instant;

use seer_core::Tree;
use seer_core::proto::{ClientMsg, PeekTarget as TargetInfo, ServerMsg, codec};
use seer_core::{TerminalCapabilities, TerminalFrame};

use super::{SharedSession, connection_loop, lock, writer};
use crate::UserSession;

pub(super) fn handle_connection(
    mut stream: UnixStream,
    shared: &SharedSession,
    connection_id: u64,
    generation: String,
    remote: bool,
) -> io::Result<()> {
    let Some(first) = greet(&mut stream, generation, remote)? else {
        return Ok(());
    };
    match first {
        ClientMsg::AttachRuntime if remote => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "a room stream cannot own this runtime",
            ));
        }
        ClientMsg::AttachRuntime => shared.add_connection(connection_id, stream.try_clone()?)?,
        ClientMsg::ObserveRuntime => {
            shared.add_view_connection(connection_id, stream.try_clone()?, None, true)?
        }
        ClientMsg::Watch {
            pane,
            cols,
            rows,
            viewer,
            ..
        } => {
            shared.add_view_connection(connection_id, stream.try_clone()?, Some(&pane), false)?;
            if let Err(error) =
                shared.watch_size(connection_id, &pane, Some(PaneSize { cols, rows }), viewer)
            {
                shared.remove_connection(connection_id)?;
                return Err(error);
            }
        }
        ClientMsg::Terminals { .. } => {
            shared.add_view_connection(connection_id, stream.try_clone()?, None, false)?
        }
        ClientMsg::QueryTargets { .. } => {
            return handle_target_query(&mut stream, shared, connection_id);
        }
        ClientMsg::QueryStatus => return super::status::handle_status_query(&mut stream, shared),
        ClientMsg::ExitClient { pane } if !remote => {
            return super::size_lease::handle_exit_client(&mut stream, shared, &pane);
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected runtime attach",
            ));
        }
    }
    let result = connection_loop(&mut stream, shared, connection_id);
    shared.remove_connection(connection_id)?;
    let _ = stream.shutdown(std::net::Shutdown::Both);
    result
}

/// The local socket sends readiness first. A room stream is opened by this
/// runtime after the broker asks for it, so the request arrives first.
fn greet(
    stream: &mut UnixStream,
    generation: String,
    remote: bool,
) -> io::Result<Option<ClientMsg>> {
    if remote {
        let Ok(first) = codec::decode(stream) else {
            return Ok(None);
        };
        codec::encode(stream, &ServerMsg::RuntimeReady { generation })?;
        return Ok(Some(first));
    }
    codec::encode(stream, &ServerMsg::RuntimeReady { generation })?;
    Ok(codec::decode(stream).ok())
}

// A burst of one poll tick can hold many pane messages; the queue and the deadline must be larger than one tick.
const OUTPUT_QUEUE_CAPACITY: usize = 64;

#[derive(Clone)]
pub(super) struct ReportedViewport {
    pub(super) workspace: String,
    pub(super) tab: String,
    pub(super) cols: u16,
    pub(super) rows: u16,
}

enum ConnectionProjection {
    Full,
    Watched(Tree),
    Catalog(BTreeSet<String>),
    End { send_bye: bool },
}

pub(super) struct Connection {
    pub(super) id: u64,
    pub(super) pid: Option<u32>,
    pub(super) watches: BTreeMap<String, PaneSize>,
    pub(super) seqs: BTreeMap<String, u64>,
    pub(super) claimed: BTreeMap<String, Instant>,
    pub(super) output: SyncSender<Arc<[u8]>>,
    pub(super) stream: UnixStream,
    pub(super) capabilities: Option<TerminalCapabilities>,
    pub(super) viewport: Option<ReportedViewport>,
    pub(super) read_only: bool,
    pub(super) catalog: bool,
    pub(super) size_owner: bool,
    pub(super) last_active: Instant,
    pub(super) watch_started: bool,
    pub(super) watch_ended: bool,
}

impl Connection {
    pub(super) fn new(id: u64, stream: UnixStream) -> io::Result<Self> {
        let writer = stream.try_clone()?;
        let (output, queued) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
        writer::spawn(id, writer, queued)?;
        Ok(Self {
            id,
            pid: super::peer::peer_pid(&stream),
            watches: BTreeMap::new(),
            seqs: BTreeMap::new(),
            claimed: BTreeMap::new(),
            output,
            stream,
            capabilities: None,
            viewport: None,
            read_only: false,
            catalog: false,
            size_owner: false,
            last_active: Instant::now(),
            watch_started: false,
            watch_ended: false,
        })
    }

    pub(super) fn send(&self, output: Arc<[u8]>) -> bool {
        self.output.try_send(output).is_ok()
    }

    pub(super) fn send_messages(&self, messages: &[ServerMsg]) -> io::Result<bool> {
        for message in messages {
            if !self.send(writer::encode(message)?) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn send_projection(
        &mut self,
        session: &UserSession,
        messages: &[ServerMsg],
        encoded: &[Arc<[u8]>],
        bye: &Arc<[u8]>,
    ) -> io::Result<bool> {
        match self.projection(session) {
            ConnectionProjection::Full => {
                for (message, output) in messages.iter().zip(encoded) {
                    let output = match self.screen(session, message) {
                        Some(screen) => writer::encode(&screen)?,
                        None => Arc::clone(output),
                    };
                    if !self.send(output) {
                        return Ok(false);
                    }
                }
            }
            ConnectionProjection::Watched(tree) => {
                for message in messages {
                    let Some(message) = project_message(&tree, message) else {
                        continue;
                    };
                    let screen = self.screen(session, &message);
                    if !self.send(writer::encode(screen.as_ref().unwrap_or(&message))?) {
                        return Ok(false);
                    }
                }
            }
            ConnectionProjection::Catalog(watched) => {
                for message in messages.iter().filter(|m| catalog_passes(&watched, m)) {
                    let screen = self.screen(session, message);
                    if !self.send(writer::encode(screen.as_ref().unwrap_or(message))?) {
                        return Ok(false);
                    }
                }
            }
            ConnectionProjection::End { send_bye } => {
                if send_bye && !self.send(Arc::clone(bye)) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    // Each watcher gets the screen wrapped to its own size (issue 406). A
    // full screen app comes at the PTY size, which grows to the largest
    // full viewer watch (issue 433). A read only link numbers the screens
    // of each watched pane, so a viewer can name the one it holds. None
    // means the shared full frame is right.
    pub(super) fn screen(
        &mut self,
        session: &UserSession,
        message: &ServerMsg,
    ) -> Option<ServerMsg> {
        let ServerMsg::Cells { pane, frame, .. } = message else {
            return None;
        };
        self.numbered(session, pane, || frame.clone())
    }

    pub(super) fn numbered(
        &mut self,
        session: &UserSession,
        pane: &str,
        full: impl FnOnce() -> TerminalFrame,
    ) -> Option<ServerMsg> {
        let size = self.watches.get(pane);
        let view = size.and_then(|size| session.pane_hosts.get(pane)?.view(*size));
        let counted = self.read_only && size.is_some();
        if view.is_none() && !counted {
            return None;
        }
        let seq = if counted {
            let seq = self.seqs.entry(pane.to_owned()).or_insert(0);
            *seq += 1;
            *seq
        } else {
            0
        };
        Some(ServerMsg::Cells {
            user: session.user.clone(),
            pane: pane.to_owned(),
            frame: view.unwrap_or_else(full),
            seq,
        })
    }

    fn projection(&mut self, session: &UserSession) -> ConnectionProjection {
        if !self.read_only || (!self.catalog && !self.watch_started) {
            return ConnectionProjection::Full;
        }
        let valid: BTreeSet<String> = self
            .watches
            .keys()
            .filter(|pane| tree_contains_pane(&session.tree, pane))
            .cloned()
            .collect();
        self.watches.retain(|pane, _| valid.contains(pane));
        self.seqs.retain(|pane, _| valid.contains(pane));
        self.claimed.retain(|pane, _| valid.contains(pane));
        if self.catalog {
            return ConnectionProjection::Catalog(valid);
        }
        if valid.is_empty() {
            if self.watch_ended {
                return ConnectionProjection::End { send_bye: false };
            }
            self.watch_ended = true;
            return ConnectionProjection::End { send_bye: true };
        }
        ConnectionProjection::Watched(project_tree(&session.tree, &valid))
    }
}

fn catalog_passes(watched: &BTreeSet<String>, message: &ServerMsg) -> bool {
    match message {
        ServerMsg::Frame { pane, .. } | ServerMsg::Cells { pane, .. } => watched.contains(pane),
        _ => true,
    }
}

fn project_message(tree: &Tree, message: &ServerMsg) -> Option<ServerMsg> {
    match message {
        ServerMsg::Tree { .. } => Some(ServerMsg::Tree { tree: tree.clone() }),
        ServerMsg::Frame { pane, .. } | ServerMsg::Cells { pane, .. }
            if !tree_contains_pane(tree, pane) =>
        {
            None
        }
        ServerMsg::Terminals { user, terminals } => Some(ServerMsg::Terminals {
            user: user.clone(),
            terminals: terminals
                .iter()
                .filter(|terminal| tree_contains_pane(tree, &terminal.pane))
                .cloned()
                .collect(),
        }),
        _ => Some(message.clone()),
    }
}

fn project_tree(tree: &Tree, watched: &BTreeSet<String>) -> Tree {
    let mut projected = tree.clone();
    for workspace in &mut projected.workspaces {
        workspace.tabs.retain_mut(|tab| {
            tab.panes.retain(|pane| watched.contains(&pane.id));
            let Some(root) = tab.layout.root.take() else {
                return false;
            };
            let Some(root) = project_layout(root, watched) else {
                return false;
            };
            tab.layout.root = Some(root);
            if !tab
                .layout
                .focused
                .as_ref()
                .is_some_and(|pane| watched.contains(pane))
            {
                tab.layout.focused = tab.panes.first().map(|pane| pane.id.clone());
            }
            true
        });
    }
    projected
        .workspaces
        .retain(|workspace| !workspace.tabs.is_empty());
    projected
}

fn project_layout(
    node: seer_core::LayoutNode,
    watched: &BTreeSet<String>,
) -> Option<seer_core::LayoutNode> {
    match node {
        seer_core::LayoutNode::Pane { pane } if watched.contains(&pane) => {
            Some(seer_core::LayoutNode::Pane { pane })
        }
        seer_core::LayoutNode::Pane { .. } => None,
        seer_core::LayoutNode::Split {
            direction,
            first,
            second,
        } => match (
            project_layout(*first, watched),
            project_layout(*second, watched),
        ) {
            (Some(first), Some(second)) => Some(seer_core::LayoutNode::Split {
                direction,
                first: Box::new(first),
                second: Box::new(second),
            }),
            (Some(node), None) | (None, Some(node)) => Some(node),
            (None, None) => None,
        },
    }
}

fn tree_contains_pane(tree: &Tree, pane: &str) -> bool {
    tree.workspaces
        .iter()
        .flat_map(|workspace| &workspace.tabs)
        .flat_map(|tab| &tab.panes)
        .any(|candidate| candidate.id == pane)
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

pub(super) fn reported_viewport(
    workspace: &str,
    tab: &str,
    cols: u16,
    rows: u16,
) -> ReportedViewport {
    ReportedViewport {
        workspace: workspace.to_owned(),
        tab: tab.to_owned(),
        cols,
        rows,
    }
}

pub(super) fn grant_next_owner(connections: &mut [Connection]) -> Option<ReportedViewport> {
    let position = connections
        .iter()
        .position(|connection| !connection.read_only)?;
    connections[position].size_owner = true;
    connections[position].viewport.clone()
}

pub(super) fn evict_connection(connections: &mut Vec<Connection>, position: usize) -> bool {
    let removed_owner = connections[position].size_owner;
    connections.remove(position);
    removed_owner
}

pub(super) fn handle_target_query(
    stream: &mut UnixStream,
    shared: &SharedSession,
    connection_id: u64,
) -> io::Result<()> {
    let _ = connection_id;
    shared.write_targets(stream)
}

impl SharedSession {
    fn add_view_connection(
        &self,
        id: u64,
        stream: UnixStream,
        pane: Option<&str>,
        catalog: bool,
    ) -> io::Result<()> {
        let _lease = lock(&self.lease)?;
        let mut messages = {
            let session = lock(&self.session)?;
            if pane.is_some_and(|id| {
                !session
                    .tree
                    .workspaces
                    .iter()
                    .flat_map(|workspace| &workspace.tabs)
                    .flat_map(|tab| &tab.panes)
                    .any(|candidate| candidate.id == id)
            }) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "terminal not found",
                ));
            }
            session.snapshot()
        };
        if catalog {
            messages.retain(|message| catalog_passes(&BTreeSet::new(), message));
        }
        let mut connection = Connection::new(id, stream)?;
        if !connection.send_messages(&messages)? {
            return Err(super::connection_closed());
        }
        // A read only connection is not one of the owner's own windows, so it has no size lease.
        connection.read_only = true;
        connection.catalog = catalog;
        seer_core::debug_log!("attach conn={id} read_only=true catalog={catalog} pane={pane:?}");
        lock(&self.connections)?.push(connection);
        self.poll_wake.notify_one();
        Ok(())
    }

    fn write_targets(&self, stream: &mut UnixStream) -> io::Result<()> {
        codec::encode(
            stream,
            &ServerMsg::Targets {
                targets: self.targets()?,
            },
        )
    }

    pub(super) fn targets(&self) -> io::Result<Vec<TargetInfo>> {
        let active = lock(&self.connections)?
            .iter()
            .find(|connection| connection.size_owner)
            .and_then(|connection| connection.viewport.as_ref())
            .map(|viewport| (viewport.workspace.clone(), viewport.tab.clone()));
        let session = lock(&self.session)?;
        Ok(session.targets(
            active
                .as_ref()
                .map(|(workspace, tab)| (workspace.as_str(), tab.as_str())),
        ))
    }
}

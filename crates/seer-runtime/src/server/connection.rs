use std::io;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc::{self, SyncSender};
use std::time::Instant;

use seer_core::TerminalCapabilities;
use seer_core::proto::{ClientMsg, PeekTarget as TargetInfo, ServerMsg, codec};
use seer_core::Tree;

use crate::UserSession;
use super::{SharedSession, connection_closed, connection_loop, lock, writer};

// A burst of one poll tick can hold many pane messages; the queue and the deadline must be larger than one tick.
const OUTPUT_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PeekTarget {
    workspace: String,
    tab: String,
}

impl PeekTarget {
    pub(super) fn new(workspace: &str, tab: &str) -> Self {
        Self {
            workspace: workspace.to_owned(),
            tab: tab.to_owned(),
        }
    }
}
#[derive(Clone)]
pub(super) struct ReportedViewport {
    pub(super) workspace: String,
    pub(super) tab: String,
    pub(super) cols: u16,
    pub(super) rows: u16,
}


enum ConnectionProjection {
    Full,
    Peek(Tree),
    End { send_bye: bool },
}

pub(super) struct Connection {
    pub(super) id: u64,
    pub(super) output: SyncSender<Arc<[u8]>>,
    pub(super) stream: UnixStream,
    pub(super) capabilities: Option<TerminalCapabilities>,
    pub(super) viewport: Option<ReportedViewport>,
    pub(super) read_only: bool,
    pub(super) size_owner: bool,
    pub(super) last_active: Instant,
    pub(super) peek_target: Option<PeekTarget>,
    pub(super) peek_ended: bool,
}

impl Connection {
    pub(super) fn new(id: u64, stream: UnixStream) -> io::Result<Self> {
        let writer = stream.try_clone()?;
        let (output, queued) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
        writer::spawn(id, writer, queued)?;
        Ok(Self {
            id,
            output,
            stream,
            capabilities: None,
            viewport: None,
            read_only: false,
            size_owner: false,
            last_active: Instant::now(),
            peek_target: None,
            peek_ended: false,
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
                for output in encoded {
                    if !self.send(Arc::clone(output)) {
                        return Ok(false);
                    }
                }
            }
            ConnectionProjection::Peek(tree) => {
                for message in messages {
                    let Some(message) = project_message(&tree, message) else {
                        continue;
                    };
                    if !self.send(writer::encode(&message)?) {
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

    fn projection(&mut self, session: &UserSession) -> ConnectionProjection {
        if self.peek_ended {
            return ConnectionProjection::End { send_bye: false };
        }
        let Some(target) = self.peek_target.as_ref() else {
            return ConnectionProjection::Full;
        };
        match session.selected_tree(&target.workspace, &target.tab) {
            Ok(tree) => ConnectionProjection::Peek(tree),
            Err(_) => {
                self.peek_target = None;
                self.peek_ended = true;
                ConnectionProjection::End { send_bye: true }
            }
        }
    }
}

fn project_message(tree: &Tree, message: &ServerMsg) -> Option<ServerMsg> {
    match message {
        ServerMsg::Tree { .. } => Some(ServerMsg::Tree {
            tree: tree.clone(),
        }),
        ServerMsg::Frame { pane, .. } | ServerMsg::Cells { pane, .. }
            if !tree_contains_pane(tree, pane) =>
        {
            None
        }
        _ => Some(message.clone()),
    }
}

fn tree_contains_pane(tree: &Tree, pane: &str) -> bool {
    tree.workspaces
        .iter()
        .flat_map(|workspace| &workspace.tabs)
        .flat_map(|tab| &tab.panes)
        .any(|candidate| candidate.id == pane)
}
pub(super) fn flush_messages(
    session: &UserSession,
    connections: &mut Vec<Connection>,
    messages: &[ServerMsg],
) -> io::Result<Option<ReportedViewport>> {
    let encoded = messages
        .iter()
        .map(writer::encode)
        .collect::<io::Result<Vec<_>>>()?;
    let bye = writer::encode(&ServerMsg::Bye {
        reason: "peek target closed".into(),
    })?;
    let owner_present = connections.iter().any(|connection| connection.size_owner);
    let mut position = 0;
    while position < connections.len() {
        if connections[position].send_projection(session, messages, &encoded, &bye)? {
            position += 1;
        } else {
            evict_connection(connections, position);
        }
    }
    if owner_present && !connections.iter().any(|connection| connection.size_owner) {
        Ok(grant_next_owner(connections))
    } else {
        Ok(None)
    }
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
    shared.write_targets(stream)?;
    let Ok(message) = codec::decode(stream) else {
        return Ok(());
    };
    let ClientMsg::Peek { workspace, tab, .. } = message else {
        return Ok(());
    };
    match shared.add_peek_connection(connection_id, stream.try_clone()?, &workspace, &tab) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
            codec::encode(
                stream,
                &ServerMsg::Refused {
                    reason: error.to_string(),
                },
            )?;
            return Ok(());
        }
        Err(error) => return Err(error),
    }
    let result = connection_loop(stream, shared, connection_id);
    shared.remove_connection(connection_id)?;
    result
}

impl SharedSession {
    fn add_peek_connection(
        &self,
        id: u64,
        stream: UnixStream,
        workspace: &str,
        tab: &str,
    ) -> io::Result<()> {
        let _lease = lock(&self.lease)?;
        let messages = {
            let session = lock(&self.session)?;
            let tree = session.selected_tree(workspace, tab)?;
            session.snapshot_for(tree)
        };
        let mut connection = Connection::new(id, stream)?;
        if !connection.send_messages(&messages)? {
            return Err(connection_closed());
        }
        connection.peek_target = Some(PeekTarget::new(workspace, tab));
        connection.read_only = true;
        lock(&self.connections)?.push(connection);
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

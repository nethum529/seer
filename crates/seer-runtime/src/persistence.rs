//! Session-level snapshot glue: the store handle, cold restore, and save.
//!
//! This module keeps UserSession small. It owns the snapshot file path and
//! revision, restores a session from a snapshot at startup, and saves the
//! session after every topology mutation.

use crate::snapshot::{self, Snapshot};
use crate::user_session::UserSession;
use seer_core::layout::PaneRect;
use seer_core::PaneSize;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

pub(crate) struct Store {
    path: std::path::PathBuf,
    revision: u64,
}

/// Loads the session snapshot from `directory`, or starts an empty session.
///
/// A missing, corrupt, unsupported, or inconsistent snapshot starts a safe
/// empty session that keeps the store, so its first shell is persisted.
/// Operational failures (directory creation, snapshot reading, metadata, or
/// replacement shell start) propagate to the caller instead of being
/// mistaken for an invalid snapshot.
pub(crate) fn load_session(
    user: String,
    shell: String,
    directory: Option<&Path>,
) -> io::Result<UserSession> {
    let Some(directory) = directory else {
        return Ok(UserSession::new(user, shell));
    };
    fs::create_dir_all(directory)?;
    let path = snapshot::snapshot_path(directory);
    match snapshot::load(&path, &user)? {
        Some(snapshot) => restore_session(user, shell, path, snapshot),
        None => Ok(empty_session(user, shell, path)),
    }
}

fn empty_session(user: String, shell: String, path: std::path::PathBuf) -> UserSession {
    let mut session = UserSession::new(user, shell);
    session.store = Some(Store {
        path,
        revision: 0,
    });
    session
}

fn restore_session(
    user: String,
    shell: String,
    path: std::path::PathBuf,
    snapshot: Snapshot,
) -> io::Result<UserSession> {
    let mut session = UserSession {
        user,
        tree: snapshot.tree,
        shell,
        viewport: snapshot.viewport,
        pane_hosts: BTreeMap::new(),
        store: Some(Store {
            path,
            revision: snapshot.revision,
        }),
    };
    for (pane, size) in pane_launches(&session) {
        let pane_rect = PaneRect {
            pane: pane.clone(),
            cols: size.cols,
            rows: size.rows,
            x: 0,
            y: 0,
        };
        let host = session.start_host(&pane_rect)?;
        session.pane_hosts.insert(pane, host);
    }
    Ok(session)
}

fn pane_launches(session: &UserSession) -> Vec<(String, PaneSize)> {
    session
        .tree
        .workspaces
        .iter()
        .flat_map(|workspace| &workspace.tabs)
        .flat_map(|tab| &tab.panes)
        .map(|pane| (pane.id.clone(), pane.size))
        .collect()
}

/// Saves the session after a successful mutation.
///
/// A failed save is an error, never a silent success: the caller reports the
/// mutation as failed instead of claiming durable state that does not exist.
pub(crate) fn persist(session: &mut UserSession) -> io::Result<()> {
    let Some(store) = session.store.as_mut() else {
        return Ok(());
    };
    store.revision = store.revision.saturating_add(1);
    let snapshot = Snapshot::capture(
        store.revision,
        &session.user,
        session.viewport,
        &session.tree,
    );
    snapshot::store(&store.path, &snapshot)
}

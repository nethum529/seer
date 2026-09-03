use crate::snapshot::{self, Snapshot};
use crate::user_session::UserSession;
use seer_core::layout::PaneRect;
use seer_core::PaneSize;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

pub(crate) struct Store {
    path: std::path::PathBuf,
    revision: u64,
}

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

/// A failed save must end the runtime so a later mutation cannot claim
/// durable state that the disk never received.
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
    snapshot::store(&store.path, &snapshot).map_err(mark_fatal)
}

#[derive(Debug)]
struct SaveFailed(io::Error);

impl fmt::Display for SaveFailed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "session snapshot save failed: {}", self.0)
    }
}

impl Error for SaveFailed {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

fn mark_fatal(error: io::Error) -> io::Error {
    io::Error::other(SaveFailed(error))
}

pub(crate) fn is_fatal(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|source| source.is::<SaveFailed>())
}

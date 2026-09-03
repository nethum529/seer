//! Versioned, atomic, bounded persistence for one user session.
//!
//! A snapshot stores the workspace tree with its pane sizes and per-tab
//! focus, the session viewport, and restart metadata. It is a declarative
//! image only: it never contains a live process or a PTY descriptor. A cold
//! restart replays the image by spawning replacement shells through the
//! normal pane start path.
//!
//! One snapshot file exists per user. Writes go to a temporary sibling file
//! and then rename over the live file, so a reader always sees either the
//! previous complete snapshot or the new one. Load rejects missing, corrupt,
//! oversized, unsupported-version, and internally inconsistent files.

use seer_core::{LayoutNode, PaneSize, Tree};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CURRENT_VERSION: u32 = 1;
const MAX_SNAPSHOT_BYTES: u64 = 4 * 1024 * 1024;
const SNAPSHOT_FILE: &str = "session.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    version: u32,
    pub(crate) revision: u64,
    saved_at: u64,
    user: String,
    pub(crate) viewport: PaneSize,
    pub(crate) tree: Tree,
}

impl Snapshot {
    #[must_use]
    pub(crate) fn capture(revision: u64, user: &str, viewport: PaneSize, tree: &Tree) -> Self {
        Self {
            version: CURRENT_VERSION,
            revision,
            saved_at: unix_seconds(),
            user: user.to_owned(),
            viewport,
            tree: tree.clone(),
        }
    }

    fn validate(&self, expected_user: &str) -> Result<(), String> {
        if self.version != CURRENT_VERSION {
            return Err(format!("unsupported snapshot version {}", self.version));
        }
        if self.user != expected_user {
            return Err("snapshot belongs to another user".into());
        }
        if self.viewport.cols == 0 || self.viewport.rows == 0 {
            return Err("snapshot viewport is empty".into());
        }
        validate_tree(&self.tree)
    }
}

#[must_use]
pub(crate) fn snapshot_path(directory: &Path) -> PathBuf {
    directory.join(SNAPSHOT_FILE)
}

/// Loads and validates the snapshot at `path` for `expected_user`.
///
/// Returns `None` for a missing, corrupt, oversized, unsupported, or
/// inconsistent file. Every rejected file is reported on stderr so a runtime
/// restart that discards a snapshot is never silent.
#[must_use]
pub(crate) fn load(path: &Path, expected_user: &str) -> Option<Snapshot> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            eprintln!("runtime cannot read the session snapshot: {error}");
            return None;
        }
    };
    let size = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            eprintln!("runtime cannot inspect the session snapshot: {error}");
            return None;
        }
    };
    if size > MAX_SNAPSHOT_BYTES {
        eprintln!("runtime discarded an oversized session snapshot ({size} bytes)");
        return None;
    }
    let snapshot: Snapshot = match serde_json::from_reader(BufReader::new(file)) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("runtime discarded a corrupt session snapshot: {error}");
            return None;
        }
    };
    match snapshot.validate(expected_user) {
        Ok(()) => Some(snapshot),
        Err(reason) => {
            eprintln!("runtime discarded an invalid session snapshot: {reason}");
            None
        }
    }
}

/// Writes `snapshot` to `path` atomically and durably.
///
/// The snapshot is serialized before any file is created, so an oversized
/// snapshot is rejected up front and never leaves a partial file behind.
pub(crate) fn store(path: &Path, snapshot: &Snapshot) -> io::Result<()> {
    let bytes = serde_json::to_vec(snapshot).map_err(json_error)?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("snapshot exceeds the {} byte limit", MAX_SNAPSHOT_BYTES),
        ));
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "snapshot has no file name"))?;
    let temporary = path.with_file_name(format!(".{}.tmp", file_name.to_string_lossy()));
    let result =
        write_snapshot_file(&temporary, &bytes).and_then(|()| fs::rename(&temporary, path));
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    sync_parent_directory(path)
}

fn write_snapshot_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(bytes)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_all()
}

/// Makes a completed rename durable across a machine reboot.
fn sync_parent_directory(path: &Path) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "snapshot has no parent directory",
        )
    })?;
    File::open(parent)?.sync_all()
}

fn json_error(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn validate_tree(tree: &Tree) -> Result<(), String> {
    let mut workspace_ids = HashSet::new();
    let mut tab_ids = HashSet::new();
    let mut pane_ids = HashSet::new();

    for workspace in &tree.workspaces {
        if workspace.id.is_empty() || !workspace_ids.insert(workspace.id.as_str()) {
            return Err(format!(
                "workspace id is empty or repeated: {}",
                workspace.id
            ));
        }
        for tab in &workspace.tabs {
            if tab.id.is_empty() || !tab_ids.insert(tab.id.as_str()) {
                return Err(format!("tab id is empty or repeated: {}", tab.id));
            }
            validate_tab(tab, &mut pane_ids)?;
        }
    }
    Ok(())
}

fn validate_tab<'a>(
    tab: &'a seer_core::Tab,
    pane_ids: &mut HashSet<&'a str>,
) -> Result<(), String> {
    if tab.panes.is_empty() {
        return Err(format!("tab has no panes: {}", tab.id));
    }
    for pane in &tab.panes {
        if pane.size.cols == 0 || pane.size.rows == 0 {
            return Err(format!("pane has an empty size: {}", pane.id));
        }
        if !pane_ids.insert(pane.id.as_str()) {
            return Err(format!("pane id is repeated: {}", pane.id));
        }
    }

    let mut leaves = Vec::new();
    if let Some(root) = tab.layout.root.as_ref() {
        collect_leaves(root, &mut leaves);
    }
    if leaves.len() != tab.panes.len() {
        return Err(format!("tab layout does not match its panes: {}", tab.id));
    }
    let pane_set = tab
        .panes
        .iter()
        .map(|pane| pane.id.as_str())
        .collect::<HashSet<_>>();
    let leaf_set = leaves.iter().copied().collect::<HashSet<_>>();
    if leaf_set != pane_set {
        return Err(format!("tab layout does not match its panes: {}", tab.id));
    }

    match tab.layout.focused.as_deref() {
        Some(pane) if pane_set.contains(pane) => Ok(()),
        Some(pane) => Err(format!("tab focus is missing from the layout: {pane}")),
        None => Err(format!("tab has panes but no focus: {}", tab.id)),
    }
}

fn collect_leaves<'a>(node: &'a LayoutNode, leaves: &mut Vec<&'a str>) {
    match node {
        LayoutNode::Pane { pane } => leaves.push(pane),
        LayoutNode::Split { first, second, .. } => {
            collect_leaves(first, leaves);
            collect_leaves(second, leaves);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_rejects_an_oversized_snapshot_without_writing_a_file() {
        let timestamp = unix_seconds();
        let directory = std::env::temp_dir().join(format!(
            "seer-snapshot-limit-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("temporary directory must be created");
        let path = directory.join(SNAPSHOT_FILE);

        let mut tree = Tree::new();
        tree.create_workspace("main")
            .expect("workspace creation must succeed");
        for _ in 0..60_000 {
            tree.create_tab("w1", "shell", PaneSize { cols: 80, rows: 24 })
                .expect("tab creation must succeed");
        }
        let snapshot = Snapshot::capture(1, "alice", PaneSize { cols: 80, rows: 24 }, &tree);

        let error = store(&path, &snapshot).expect_err("oversized snapshot must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!path.exists(), "snapshot file must not be written");
        let temporary = path.with_file_name(format!(".{}.tmp", SNAPSHOT_FILE));
        assert!(!temporary.exists(), "temporary file must not remain behind");

        fs::remove_dir_all(&directory).expect("temporary directory must be removed");
    }

    #[test]
    fn validate_rejects_a_tab_without_panes() {
        let mut tree = Tree::new();
        tree.create_workspace("main")
            .expect("workspace creation must succeed");
        tree.create_tab("w1", "shell", PaneSize { cols: 80, rows: 24 })
            .expect("tab creation must succeed");
        tree.close_pane("w1:p1").expect("pane close must succeed");

        let snapshot = Snapshot::capture(1, "alice", PaneSize { cols: 80, rows: 24 }, &tree);
        let error = snapshot
            .validate("alice")
            .expect_err("empty tab must be rejected");
        assert!(error.contains("no panes"));
    }
}

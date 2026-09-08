use std::collections::BTreeMap;
use std::io;

use seer_core::PaneSize;

use super::{SharedSession, connection::Connection, lock};

impl SharedSession {
    pub(super) fn watch_size(
        &self,
        id: u64,
        pane: &str,
        size: Option<PaneSize>,
    ) -> io::Result<bool> {
        let lease = lock(&self.lease)?;
        let valid = lock(&self.session)?.pane_hosts.contains_key(pane)
            && size.is_none_or(|size| size.cols > 0 && size.rows > 0);
        if !valid {
            drop(lease);
            self.send_refused(id, "terminal or size is invalid".into())?;
            return Ok(false);
        }
        {
            let mut connections = lock(&self.connections)?;
            let Some(connection) = connections
                .iter_mut()
                .find(|connection| connection.id == id)
            else {
                return Ok(true);
            };
            if let Some(size) = size {
                connection.watches.insert(pane.into(), size);
                connection.watch_started = true;
                connection.watch_ended = false;
            } else {
                connection.watches.remove(pane);
                if connection.watches.is_empty() {
                    connection.watch_started = false;
                    connection.watch_ended = false;
                }
            }
        }
        self.flush_messages(&[])?;
        Ok(false)
    }

    pub(super) fn visible_sizes(connections: &[Connection]) -> BTreeMap<String, PaneSize> {
        let mut sizes: BTreeMap<String, PaneSize> = BTreeMap::new();
        for connection in connections {
            for (pane, size) in &connection.watches {
                sizes
                    .entry(pane.clone())
                    .and_modify(|smallest| {
                        smallest.cols = smallest.cols.min(size.cols);
                        smallest.rows = smallest.rows.min(size.rows);
                    })
                    .or_insert(*size);
            }
        }
        sizes
    }
}

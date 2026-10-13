use super::*;

use std::os::unix::net::UnixStream;
use std::time::Duration;

async fn inspect(writer: Writer, pane: String) -> io::Result<Vec<String>> {
    let path = match &*lock(&writer) {
        Socket::Iroh(stream) => stream
            .peer_addr()?
            .as_pathname()
            .map(std::path::Path::to_owned),
        Socket::Tcp(_) => None,
    }
    .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "local runtime socket required"))?;
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let _: ServerMsg = codec::decode(&mut stream)?;
    codec::encode(&mut stream, &ClientMsg::InspectTerminal { pane })?;
    loop {
        match codec::decode(&mut stream)? {
            ServerMsg::TerminalPrograms { programs } => return Ok(programs),
            ServerMsg::Refused { reason } | ServerMsg::Bye { reason } => {
                return Err(io::Error::other(reason));
            }
            _ => {}
        }
    }
}

impl Link {
    pub(crate) fn inspect_terminal(
        &self,
        pane: String,
    ) -> impl Future<Output = io::Result<Vec<String>>> + Send + use<> {
        inspect(Arc::clone(&self.writer), pane)
    }
    pub(crate) fn rename(&mut self, pane: &str, title: String) -> io::Result<()> {
        if self.remote() {
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
        let user = self.user.clone();
        self.select_person(&user)?;
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
            if self.pending_new.is_some() {
                return Ok(());
            }
            self.pending_new = Some(
                self.tree
                    .workspaces
                    .iter()
                    .flat_map(|workspace| &workspace.tabs)
                    .flat_map(|tab| &tab.panes)
                    .map(|pane| pane.id.clone())
                    .collect(),
            );
            self.send(&ClientMsg::CreateTab { workspace })?;
        }
        Ok(())
    }

    pub(crate) fn close_terminal(&mut self, pane: &str) -> io::Result<()> {
        if self.remote() {
            return Ok(());
        }
        let Some(target) = pane_target(&self.tree, pane) else {
            return Ok(());
        };
        self.send(&ClientMsg::ClosePane {
            workspace: target.workspace,
            tab: target.tab,
            pane: target.pane,
        })
    }

    pub(super) fn retarget(&mut self, tree: &Tree) -> io::Result<()> {
        let panes: Vec<_> = tree
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.tabs)
            .flat_map(|tab| &tab.panes)
            .map(|pane| pane.id.as_str())
            .collect();
        if let Some(existing) = &self.pending_new
            && let Some(pane) = panes
                .iter()
                .find(|pane| !existing.iter().any(|old| old == **pane))
        {
            self.pending_new = None;
            self.show(pane_target(tree, pane))?;
            for event in std::mem::take(&mut self.pending_input) {
                self.input_local(event)?;
            }
            return Ok(());
        }
        if self
            .target
            .as_ref()
            .is_some_and(|target| panes.contains(&target.pane.as_str()))
        {
            return Ok(());
        }
        let at = self
            .target
            .as_ref()
            .and_then(|target| {
                self.terminals
                    .iter()
                    .position(|terminal| terminal.pane == target.pane)
            })
            .unwrap_or(0);
        let target = panes
            .get(at.min(panes.len().saturating_sub(1)))
            .and_then(|pane| pane_target(tree, pane));
        self.show(target)
    }
}

use std::os::unix::net::UnixStream;

use seer_core::proto::ClientMsg;

pub(crate) struct Location {
    pub(crate) workspace: String,
    pub(crate) tab: String,
    pub(crate) pane: String,
}

pub(crate) fn first_terminal(tree: &seer_core::Tree) -> Location {
    let workspace = &tree.workspaces[0];
    let tab = &workspace.tabs[0];
    Location {
        workspace: workspace.id.clone(),
        tab: tab.id.clone(),
        pane: tab.panes[0].id.clone(),
    }
}

pub(crate) fn type_locally(window: &mut UnixStream, at: &Location, text: &str) {
    super::room::send(
        window,
        &ClientMsg::TerminalInput {
            workspace: at.workspace.clone(),
            tab: at.tab.clone(),
            pane: at.pane.clone(),
            input: seer_core::TerminalInput::new(seer_core::InputEvent::Text(text.to_owned())),
        },
    );
}

pub(crate) fn frame_text(frame: &seer_core::TerminalFrame) -> String {
    frame
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| cell.character).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

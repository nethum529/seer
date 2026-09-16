pub mod codec;
pub(crate) mod frame_rows;

use serde::{Deserialize, Serialize};

use crate::{SplitDirection, TerminalCapabilities, TerminalFrame, TerminalInput, Tree};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClientMsg {
    Hello {
        user_id: String,
        credential: String,
        version: String,
    },
    Join {
        seat_token: String,
        name: String,
    },
    Invite {
        hours: Option<u32>,
    },
    ListPeople,
    QueryStatus,
    DetachClient {
        client_id: String,
    },
    ExitClient {
        pane: String,
    },
    CreateTab {
        workspace: String,
    },
    SplitPane {
        workspace: String,
        tab: String,
        direction: SplitDirection,
    },
    ClosePane {
        workspace: String,
        tab: String,
        pane: String,
    },
    FocusPane {
        workspace: String,
        tab: String,
        pane: String,
    },
    TerminalCapabilities {
        capabilities: TerminalCapabilities,
    },
    TerminalInput {
        workspace: String,
        tab: String,
        pane: String,
        input: TerminalInput,
    },
    Resize {
        workspace: String,
        tab: String,
        cols: u16,
        rows: u16,
    },
    GrantedMouse {
        workspace: String,
        tab: String,
        pane: String,
        mouse: crate::MouseInput,
        sender: String,
    },
    GrantedInput {
        workspace: String,
        tab: String,
        pane: String,
        bytes: Vec<u8>,
        sender: String,
    },
    AttachRuntime,
    ObserveRuntime,
    PublishRuntime {
        user_id: String,
        credential: String,
        version: String,
        generation: String,
    },
    RuntimeStream {
        user_id: String,
        credential: String,
        token: String,
    },
    QueryTargets {
        user: String,
    },
    Watch {
        user: String,
        pane: String,
        cols: u16,
        rows: u16,
    },
    Unwatch {
        user: String,
        pane: String,
    },
    Terminals {
        user: String,
    },
    MouseInto {
        user: String,
        pane: String,
        mouse: crate::MouseInput,
    },
    TypeInto {
        user: String,
        pane: String,
        bytes: Vec<u8>,
    },
    SetAllGrants {
        can_type: bool,
    },
    SetGrant {
        user: String,
        can_type: bool,
    },
    Stop,
    Leave,
    Detach,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ServerMsg {
    GrantsUpdated,
    Terminals {
        user: String,
        terminals: Vec<TerminalInfo>,
    },
    Presence {
        user: String,
        online: bool,
        idle_secs: u64,
    },
    Grants {
        can_type_here: Vec<String>,
        you_may_type_into: Vec<String>,
    },
    RuntimeReady {
        generation: String,
    },
    Published {
        generation: String,
    },
    OpenStream {
        token: String,
    },
    Welcome {
        user_id: String,
        name: String,
        client_id: String,
        tree: Tree,
    },
    Joined {
        user_id: String,
        credential: String,
        name: String,
    },
    Seat {
        capsule: String,
        expires_in_secs: u64,
    },
    People {
        people: Vec<Person>,
    },
    Status {
        tabs: u32,
        foreground: String,
        idle_secs: u64,
        #[serde(default)]
        windows: Option<Vec<u32>>,
        #[serde(default)]
        shells: Option<u32>,
    },
    Clients {
        clients: Vec<ClientInfo>,
    },
    Targets {
        targets: Vec<PeekTarget>,
    },
    Refused {
        reason: String,
    },
    Tree {
        tree: Tree,
    },
    Frame {
        pane: String,
        bytes: Vec<u8>,
    },
    Cells {
        user: String,
        pane: String,
        frame: TerminalFrame,
    },
    Bye {
        reason: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PeekTarget {
    pub workspace: String,
    pub workspace_name: String,
    pub tab: String,
    pub tab_title: String,
    pub active: bool,
}

impl ClientMsg {
    #[must_use]
    pub fn is_mutating(&self) -> bool {
        matches!(
            self,
            Self::TerminalInput { .. }
                | Self::CreateTab { .. }
                | Self::SplitPane { .. }
                | Self::ClosePane { .. }
                | Self::FocusPane { .. }
                | Self::Resize { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum PersonState {
    Active,
    Idle,
    #[default]
    Away,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Person {
    #[serde(default)]
    pub online: bool,
    pub user_id: String,
    pub name: String,
    pub attached_clients: u32,
    pub peekable: bool,
    #[serde(default)]
    pub host: bool,
    #[serde(default)]
    pub state: PersonState,
    #[serde(default)]
    pub tabs: u32,
    #[serde(default)]
    pub foreground: String,
    #[serde(default)]
    pub idle_secs: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientInfo {
    pub client_id: String,
    pub connected_secs: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalInfo {
    #[serde(default)]
    pub last_typist: Option<String>,
    pub pane: String,
    pub name: String,
    pub state: String,
    pub cols: u16,
    pub rows: u16,
}

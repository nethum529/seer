pub mod codec;

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
    AttachRuntime,
    QueryTargets {
        user: String,
    },
    Peek {
        user: String,
        workspace: String,
        tab: String,
    },
    StopPeek,
    Detach,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ServerMsg {
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
    pub user_id: String,
    pub name: String,
    pub attached_clients: u32,
    pub peekable: bool,
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

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use serde::{Serialize, de::DeserializeOwned};

    use super::{ClientInfo, ClientMsg, PeekTarget, Person, PersonState, ServerMsg, codec};
    use crate::{
        Cell, Color, Cursor, InputEvent, KeyCode, KeyInput, Modifiers, PaneSize, SplitDirection,
        TERMINAL_PROTOCOL_VERSION, TerminalCapabilities, TerminalFrame, TerminalInput,
        TerminalModes, Tree,
    };

    fn assert_round_trip<T>(message: &T)
    where
        T: Debug + DeserializeOwned + PartialEq + Serialize,
    {
        let mut bytes = Vec::new();
        codec::encode(&mut bytes, message).expect("message must encode");
        let decoded = codec::decode(&mut bytes.as_slice()).expect("message must decode");
        assert_eq!(message, &decoded);
    }

    fn tree_with_two_panes() -> Tree {
        let mut tree = Tree::new();
        tree.create_workspace("main")
            .expect("workspace must be created");
        tree.create_tab(
            "w1",
            "shell",
            PaneSize {
                cols: 120,
                rows: 40,
            },
        )
        .expect("tab must be created");
        tree.split_pane("w1:p1", SplitDirection::Right)
            .expect("pane must be split");
        tree
    }

    #[test]
    fn client_messages_round_trip() {
        let messages = [
            ClientMsg::Hello {
                user_id: "user-1".into(),
                credential: "credential-1".into(),
                version: "0.1.0".into(),
            },
            ClientMsg::Join {
                seat_token: "seat-1".into(),
                name: "Alice".into(),
            },
            ClientMsg::Invite { hours: None },
            ClientMsg::ListPeople,
            ClientMsg::QueryStatus,
            ClientMsg::DetachClient {
                client_id: "client-1".into(),
            },
            ClientMsg::CreateTab {
                workspace: "w1".into(),
            },
            ClientMsg::SplitPane {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                direction: SplitDirection::Right,
            },
            ClientMsg::SplitPane {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                direction: SplitDirection::Down,
            },
            ClientMsg::ClosePane {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                pane: "w1:p1".into(),
            },
            ClientMsg::FocusPane {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                pane: "w1:p2".into(),
            },
            ClientMsg::TerminalCapabilities {
                capabilities: TerminalCapabilities {
                    protocol_version: TERMINAL_PROTOCOL_VERSION,
                },
            },
            ClientMsg::TerminalInput {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                pane: "w1:p1".into(),
                input: TerminalInput::new(InputEvent::Key(KeyInput {
                    code: KeyCode::Function(5),
                    modifiers: Modifiers::default(),
                })),
            },
            ClientMsg::Resize {
                workspace: "w1".into(),
                tab: "w1:t1".into(),
                cols: 120,
                rows: 40,
            },
            ClientMsg::AttachRuntime,
            ClientMsg::QueryTargets { user: "bob".into() },
            ClientMsg::Peek {
                user: "bob".into(),
                workspace: "w1".into(),
                tab: "w1:t1".into(),
            },
            ClientMsg::StopPeek,
            ClientMsg::Detach,
        ];

        for message in messages {
            assert_round_trip(&message);
        }
    }

    #[test]
    fn server_messages_round_trip() {
        let tree = tree_with_two_panes();
        let messages = [
            ServerMsg::Welcome {
                user_id: "user-1".into(),
                name: "Alice".into(),
                client_id: "client-1".into(),
                tree: tree.clone(),
            },
            ServerMsg::Joined {
                user_id: "user-2".into(),
                credential: "credential-2".into(),
                name: "Bob".into(),
            },
            ServerMsg::Seat {
                capsule: "capsule-1".into(),
                expires_in_secs: 3_600,
            },
            ServerMsg::People {
                people: vec![Person {
                    user_id: "user-1".into(),
                    name: "Alice".into(),
                    attached_clients: 2,
                    peekable: true,
                    state: PersonState::Active,
                    tabs: 3,
                    foreground: "nvim".into(),
                    idle_secs: 12,
                }],
            },
            ServerMsg::Targets {
                targets: vec![PeekTarget {
                    workspace: "w2".into(),
                    workspace_name: "work".into(),
                    tab: "w2:t3".into(),
                    tab_title: "shell".into(),
                    active: true,
                }],
            },
            ServerMsg::Clients {
                clients: vec![ClientInfo {
                    client_id: "client-1".into(),
                    connected_secs: 60,
                }],
            },
            ServerMsg::Refused {
                reason: "invalid token".into(),
            },
            ServerMsg::Tree { tree },
            ServerMsg::Frame {
                pane: "w1:p1".into(),
                bytes: vec![0, 1, 255],
            },
            ServerMsg::Cells {
                pane: "w1:p1".into(),
                frame: TerminalFrame {
                    rows: vec![
                        vec![Cell {
                            character: 'A',
                            fg: Color::Indexed(1),
                            bg: Color::Default,
                            bold: true,
                            italic: false,
                            underline: false,
                            dim: false,
                            inverse: false,
                            hidden: false,
                            strikeout: false,
                        }],
                        vec![Cell {
                            character: 'B',
                            fg: Color::Rgb {
                                red: 10,
                                green: 20,
                                blue: 30,
                            },
                            bg: Color::Indexed(2),
                            bold: false,
                            italic: true,
                            underline: true,
                            dim: false,
                            inverse: false,
                            hidden: false,
                            strikeout: false,
                        }],
                    ],
                    cursor: Cursor::default(),
                    modes: TerminalModes::default(),
                },
            },
            ServerMsg::Bye {
                reason: "detached".into(),
            },
        ];

        for message in messages {
            assert_round_trip(&message);
        }
    }
}

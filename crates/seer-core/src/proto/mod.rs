pub mod codec;

use serde::{Deserialize, Serialize};

use crate::{Cell, SplitDirection, Tree};

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
    DetachClient {
        client_id: String,
    },
    CreateTab,
    SplitPane {
        direction: SplitDirection,
    },
    ClosePane {
        pane: String,
    },
    FocusPane {
        pane: String,
    },
    Input {
        pane: String,
        bytes: Vec<u8>,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    Peek {
        user: String,
        workspace: String,
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
        rows: Vec<Vec<Cell>>,
    },
    Bye {
        reason: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Person {
    pub user_id: String,
    pub name: String,
    pub attached_clients: u32,
    pub peekable: bool,
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

    use super::{ClientInfo, ClientMsg, Person, ServerMsg, codec};
    use crate::{Cell, Color, PaneSize, SplitDirection, Tree};

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
            ClientMsg::DetachClient {
                client_id: "client-1".into(),
            },
            ClientMsg::CreateTab,
            ClientMsg::SplitPane {
                direction: SplitDirection::Right,
            },
            ClientMsg::SplitPane {
                direction: SplitDirection::Down,
            },
            ClientMsg::ClosePane {
                pane: "w1:p1".into(),
            },
            ClientMsg::FocusPane {
                pane: "w1:p2".into(),
            },
            ClientMsg::Input {
                pane: "w1:p1".into(),
                bytes: vec![0, 1, 255],
            },
            ClientMsg::Resize {
                cols: 120,
                rows: 40,
            },
            ClientMsg::Peek {
                user: "bob".into(),
                workspace: "w1".into(),
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

pub mod codec;

use serde::{Deserialize, Serialize};

use crate::{SplitDirection, Tree};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClientMsg {
    Hello { user: String, token: String },
    CreateTab,
    SplitPane { direction: SplitDirection },
    ClosePane { pane: String },
    FocusPane { pane: String },
    Input { pane: String, bytes: Vec<u8> },
    Resize { cols: u16, rows: u16 },
    Peek { user: String, workspace: String },
    StopPeek,
    Detach,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ServerMsg {
    Welcome { user: String, tree: Tree },
    Refused { reason: String },
    Tree { tree: Tree },
    Frame { pane: String, bytes: Vec<u8> },
    Bye { reason: String },
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use serde::{Serialize, de::DeserializeOwned};

    use super::{ClientMsg, ServerMsg, codec};
    use crate::{PaneSize, SplitDirection, Tree};

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
                user: "alice".into(),
                token: "secret".into(),
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
                user: "alice".into(),
                tree: tree.clone(),
            },
            ServerMsg::Refused {
                reason: "invalid token".into(),
            },
            ServerMsg::Tree { tree },
            ServerMsg::Frame {
                pane: "w1:p1".into(),
                bytes: vec![0, 1, 255],
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

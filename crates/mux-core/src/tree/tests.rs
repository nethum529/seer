use super::*;

const SIZE: PaneSize = PaneSize { cols: 80, rows: 24 };

fn tree_with_tab() -> Tree {
    let mut tree = Tree::new();
    let workspace = tree
        .create_workspace("main")
        .expect("workspace creation must succeed");
    tree.create_tab(&workspace.id, "shell", SIZE)
        .expect("tab creation must succeed");
    tree
}

#[test]
fn creates_workspaces_with_stable_ids() {
    let mut tree = Tree::new();

    let first = tree
        .create_workspace("first")
        .expect("workspace creation must succeed");
    let second = tree
        .create_workspace("second")
        .expect("workspace creation must succeed");

    assert_eq!(first.id, "w1");
    assert_eq!(first.name, "first");
    assert!(first.tabs.is_empty());
    assert_eq!(second.id, "w2");
    assert_eq!(tree.workspaces, vec![first, second]);
}

#[test]
fn creates_a_tab_with_its_first_pane() {
    let mut tree = Tree::new();
    let workspace = tree
        .create_workspace("main")
        .expect("workspace creation must succeed");

    let tab = tree
        .create_tab(&workspace.id, "shell", SIZE)
        .expect("tab creation must succeed");

    assert_eq!(tab.id, "w1:t1");
    assert_eq!(tab.title, "shell");
    assert_eq!(
        tab.panes,
        vec![Pane {
            id: "w1:p1".into(),
            size: SIZE
        }]
    );
    assert_eq!(tab.layout.focused.as_deref(), Some("w1:p1"));
    assert_eq!(
        tab.layout.root,
        Some(LayoutNode::Pane {
            pane: "w1:p1".into()
        })
    );
}

#[test]
fn reports_a_missing_workspace_when_creating_a_tab() {
    let mut tree = Tree::new();

    let result = tree.create_tab("w9", "shell", SIZE);

    assert_eq!(result, Err(TreeError::WorkspaceNotFound("w9".into())));
}

#[test]
fn splits_panes_right_and_down() {
    let mut tree = tree_with_tab();

    let right = tree
        .split_pane("w1:p1", SplitDirection::Right)
        .expect("right split must succeed");
    assert_eq!(right.panes[1].id, "w1:p2");
    assert_eq!(right.panes[1].size, SIZE);
    assert_eq!(right.layout.focused.as_deref(), Some("w1:p2"));

    let down = tree
        .split_pane("w1:p2", SplitDirection::Down)
        .expect("down split must succeed");
    assert_eq!(down.panes[2].id, "w1:p3");
    assert_eq!(down.layout.focused.as_deref(), Some("w1:p3"));
    assert!(matches!(
        down.layout.root,
        Some(LayoutNode::Split {
            direction: SplitDirection::Right,
            second,
            ..
        }) if matches!(
            *second,
            LayoutNode::Split {
                direction: SplitDirection::Down,
                ..
            }
        )
    ));
}

#[test]
fn closing_the_focused_pane_focuses_its_sibling() {
    let mut tree = tree_with_tab();
    tree.split_pane("w1:p1", SplitDirection::Right)
        .expect("split must succeed");
    tree.split_pane("w1:p2", SplitDirection::Down)
        .expect("split must succeed");

    let tab = tree.close_pane("w1:p3").expect("pane close must succeed");

    assert_eq!(tab.layout.focused.as_deref(), Some("w1:p2"));
    assert_eq!(tab.panes.len(), 2);
    assert!(!tab.panes.iter().any(|pane| pane.id == "w1:p3"));
}

#[test]
fn closing_an_unfocused_pane_keeps_focus() {
    let mut tree = tree_with_tab();
    tree.split_pane("w1:p1", SplitDirection::Right)
        .expect("split must succeed");

    let tab = tree.close_pane("w1:p1").expect("pane close must succeed");

    assert_eq!(tab.layout.focused.as_deref(), Some("w1:p2"));
    assert_eq!(
        tab.layout.root,
        Some(LayoutNode::Pane {
            pane: "w1:p2".into()
        })
    );
}

#[test]
fn closing_the_last_pane_leaves_an_empty_tab() {
    let mut tree = tree_with_tab();

    let tab = tree.close_pane("w1:p1").expect("pane close must succeed");

    assert!(tab.panes.is_empty());
    assert_eq!(tab.layout.root, None);
    assert_eq!(tab.layout.focused, None);
}

#[test]
fn focuses_an_existing_pane() {
    let mut tree = tree_with_tab();
    tree.split_pane("w1:p1", SplitDirection::Right)
        .expect("split must succeed");

    let tab = tree.focus_pane("w1:p1").expect("pane focus must succeed");

    assert_eq!(tab.layout.focused.as_deref(), Some("w1:p1"));
}

#[test]
fn pane_ids_are_not_reused_after_close() {
    let mut tree = tree_with_tab();
    tree.split_pane("w1:p1", SplitDirection::Right)
        .expect("split must succeed");
    tree.close_pane("w1:p2").expect("pane close must succeed");

    let tab = tree
        .split_pane("w1:p1", SplitDirection::Down)
        .expect("second split must succeed");

    assert_eq!(tab.panes[1].id, "w1:p3");
}

#[test]
fn splitting_a_closed_pane_returns_an_error() {
    let mut tree = tree_with_tab();
    tree.split_pane("w1:p1", SplitDirection::Right)
        .expect("split must succeed");
    tree.close_pane("w1:p2").expect("pane close must succeed");

    let result = tree.split_pane("w1:p2", SplitDirection::Down);

    assert_eq!(result, Err(TreeError::PaneNotFound("w1:p2".into())));
}

#[test]
fn close_and_focus_report_a_missing_pane() {
    let mut tree = tree_with_tab();

    assert_eq!(
        tree.close_pane("w1:p9"),
        Err(TreeError::PaneNotFound("w1:p9".into()))
    );
    assert_eq!(
        tree.focus_pane("w1:p9"),
        Err(TreeError::PaneNotFound("w1:p9".into()))
    );
}

#[test]
fn nested_layout_removal_rebuilds_the_parent_split() {
    let mut tree = tree_with_tab();
    tree.split_pane("w1:p1", SplitDirection::Right)
        .expect("split must succeed");
    tree.split_pane("w1:p1", SplitDirection::Down)
        .expect("split must succeed");
    tree.focus_pane("w1:p1").expect("focus must succeed");

    let tab = tree.close_pane("w1:p1").expect("close must succeed");

    assert_eq!(tab.layout.focused.as_deref(), Some("w1:p3"));
    assert!(matches!(
        tab.layout.root,
        Some(LayoutNode::Split {
            direction: SplitDirection::Right,
            first,
            second,
        }) if matches!(*first, LayoutNode::Pane { ref pane } if pane == "w1:p3")
            && matches!(*second, LayoutNode::Pane { ref pane } if pane == "w1:p2")
    ));
}

#[test]
fn closing_next_to_a_split_focuses_its_first_pane() {
    let mut tree = tree_with_tab();
    tree.split_pane("w1:p1", SplitDirection::Right)
        .expect("split must succeed");
    tree.split_pane("w1:p2", SplitDirection::Down)
        .expect("split must succeed");
    tree.focus_pane("w1:p1").expect("focus must succeed");

    let tab = tree.close_pane("w1:p1").expect("close must succeed");

    assert_eq!(tab.layout.focused.as_deref(), Some("w1:p2"));
}

#[test]
fn operations_reject_a_pane_missing_from_the_layout() {
    let mut split_tree = tree_with_tab();
    split_tree.workspaces[0].tabs[0].layout.root = Some(LayoutNode::Pane {
        pane: "other".into(),
    });
    assert_eq!(
        split_tree.split_pane("w1:p1", SplitDirection::Right),
        Err(TreeError::PaneNotFound("w1:p1".into()))
    );

    let mut close_tree = split_tree.clone();
    assert_eq!(
        close_tree.close_pane("w1:p1"),
        Err(TreeError::PaneNotFound("w1:p1".into()))
    );
    assert_eq!(
        split_tree.focus_pane("w1:p1"),
        Err(TreeError::PaneNotFound("w1:p1".into()))
    );
}

#[test]
fn removing_from_an_empty_or_different_layout_returns_false() {
    let mut empty = Layout {
        root: None,
        focused: None,
    };
    assert!(!empty.remove("w1:p1"));

    let mut different = Layout::from_pane("w1:p1");
    let original = different.clone();
    assert!(!different.remove("w1:p2"));
    assert_eq!(different, original);
}

#[test]
fn reports_id_exhaustion() {
    let mut tree = Tree {
        workspaces: Vec::new(),
        next_workspace_id: u64::MAX,
    };
    assert_eq!(tree.create_workspace("main"), Err(TreeError::IdExhausted));

    let mut tree = tree_with_tab();
    tree.workspaces[0].next_pane_id = u64::MAX;
    assert_eq!(
        tree.split_pane("w1:p1", SplitDirection::Right),
        Err(TreeError::IdExhausted)
    );

    tree.workspaces[0].next_tab_id = u64::MAX;
    assert_eq!(
        tree.create_tab("w1", "second", SIZE),
        Err(TreeError::IdExhausted)
    );
}

#[test]
fn tree_errors_have_direct_messages() {
    assert_eq!(
        TreeError::WorkspaceNotFound("w2".into()).to_string(),
        "workspace not found: w2"
    );
    assert_eq!(
        TreeError::PaneNotFound("w1:p2".into()).to_string(),
        "pane not found: w1:p2"
    );
    assert_eq!(
        TreeError::IdExhausted.to_string(),
        "tree id space is exhausted"
    );
}

#[test]
fn model_types_implement_serde_traits() {
    fn assert_serde<T: Serialize + for<'de> Deserialize<'de>>() {}

    assert_serde::<Tree>();
    assert_serde::<Workspace>();
    assert_serde::<Tab>();
    assert_serde::<Pane>();
    assert_serde::<PaneSize>();
    assert_serde::<Layout>();
    assert_serde::<LayoutNode>();
    assert_serde::<SplitDirection>();
    assert_serde::<TreeError>();
}

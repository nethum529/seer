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

    let tab = tree
        .create_tab("w1", "shell", SIZE)
        .expect("tab creation must succeed");
    assert_eq!(
        (tab.id.as_str(), tab.panes[0].id.as_str()),
        ("w1:t1", "w1:p1")
    );
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

    tree.close_pane("w1:p3").expect("pane close must succeed");
    let split = tree
        .split_pane("w1:p2", SplitDirection::Down)
        .expect("split after close must succeed");
    assert_ne!(split.panes[2].id, "w1:p3");
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

    let tab = tree.close_pane("w1:p1").expect("pane close must succeed");
    assert_eq!(tab.layout.focused.as_deref(), Some("w1:p2"));

    let tab = tree.close_pane("w1:p2").expect("pane close must succeed");
    assert!(tab.panes.is_empty());
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

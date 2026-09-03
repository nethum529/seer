use std::cell::RefCell;

use ratatui::layout::Rect;
use seer_core::Tree;

use crate::input::{
    FocusDirection, pane_in_direction as find_pane_in_direction, selected_tab_tree,
};
use crate::state::{ClientState, pane_rects};

const STATUS_HINT: &str = "Ctrl-b c new tab, % split, x close, n/p tabs";
const LAST_TAB_STATUS: &str = "Cannot close the last tab.";

thread_local! {
    static NAVIGATION_TREE: RefCell<Option<Tree>> = const { RefCell::new(None) };
    static STATUS: RefCell<&'static str> = const { RefCell::new(STATUS_HINT) };
}

pub(crate) fn initialize(tree: &Tree) {
    update_tree(tree);
    show_hint();
}

pub(crate) fn update_tree(tree: &Tree) {
    NAVIGATION_TREE.with_borrow_mut(|current| *current = Some(tree.clone()));
}

pub(crate) fn show_hint() {
    STATUS.with_borrow_mut(|current| *current = STATUS_HINT);
}

pub(crate) fn show_last_tab_status() {
    STATUS.with_borrow_mut(|current| *current = LAST_TAB_STATUS);
}

pub(crate) fn status() -> &'static str {
    STATUS.with_borrow(|status| *status)
}

pub(crate) fn select_tab(state: &mut ClientState, forward: bool) {
    let Some((workspace, tab)) = state
        .selection()
        .map(|(workspace, tab)| (workspace.to_owned(), tab.to_owned()))
    else {
        return;
    };
    let selected = NAVIGATION_TREE.with_borrow(|tree| {
        tree.as_ref()
            .and_then(|tree| selected_tab_tree(tree, &workspace, &tab, forward))
    });
    if let Some(tree) = selected {
        state.replace_tree(tree);
    }
}

pub(crate) fn closes_last_tab(state: &ClientState) -> bool {
    let Some((workspace, _)) = state.selection() else {
        return false;
    };
    let last_pane = state.visible_tab().is_some_and(|tab| tab.panes.len() == 1);
    last_pane
        && NAVIGATION_TREE.with_borrow(|tree| {
            tree.as_ref().is_none_or(|tree| {
                tree.workspaces
                    .iter()
                    .find(|candidate| candidate.id == workspace)
                    .is_none_or(|workspace| workspace.tabs.len() == 1)
            })
        })
}

pub(crate) fn pane_in_direction(
    state: &ClientState,
    direction: FocusDirection,
) -> Option<String> {
    let tab = state.visible_tab()?;
    let focused = state.focused()?;
    let rects = pane_rects(tab, Rect::new(0, 0, 1_000, 1_000));
    find_pane_in_direction(&rects, focused, direction)
}

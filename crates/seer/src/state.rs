use ratatui::layout::Rect;
use seer_core::proto::{Person, PersonState, TerminalInfo};
use seer_core::{Cursor, TerminalFrame, Tree};
use std::collections::{BTreeSet, HashMap};

pub(crate) struct ClientState {
    pub(crate) tree: Tree,
    pub(crate) own_user: String,
    pub(crate) own_name: String,
    pub(crate) server: String,
    pub(crate) people: Vec<Person>,
    pub(crate) selected: usize,
    pub(crate) focus: usize,
    pub(crate) terminals: HashMap<String, Vec<TerminalInfo>>,
    pub(crate) frames: HashMap<(String, String), TerminalFrame>,
    pub(crate) viewer: Option<(String, String)>,
    pub(crate) people_areas: Vec<(usize, Rect)>,
    pub(crate) box_areas: Vec<(usize, Rect)>,
    pub(crate) people_scroll: usize,
    pub(crate) grid_scroll: usize,
    pub(crate) search: String,
    pub(crate) searching: bool,
    pub(crate) quit_prompt: bool,
    pub(crate) invite: Option<String>,
    pub(crate) invite_pending: bool,
    pub(crate) notice: String,
    pub(crate) watches: BTreeSet<(String, String)>,
    pub(crate) pending_new: Option<BTreeSet<String>>,
}

impl ClientState {
    pub(crate) fn new(tree: Tree, own_user: String) -> Self {
        let own = Person {
            user_id: own_user.clone(),
            name: own_user.clone(),
            online: true,
            idle_secs: 0,
            attached_clients: 1,
            peekable: true,
            state: PersonState::Active,
            tabs: 0,
            foreground: String::new(),
        };
        Self {
            tree,
            own_name: own_user.clone(),
            own_user,
            server: String::new(),
            people: vec![own],
            selected: 0,
            focus: 0,
            terminals: HashMap::new(),
            frames: HashMap::new(),
            viewer: None,
            people_areas: Vec::new(),
            box_areas: Vec::new(),
            people_scroll: 0,
            grid_scroll: 0,
            search: String::new(),
            searching: false,
            quit_prompt: false,
            invite: None,
            invite_pending: false,
            notice: String::new(),
            watches: BTreeSet::new(),
            pending_new: None,
        }
    }

    pub(crate) fn user(&self) -> &str {
        self.people
            .get(self.selected)
            .map_or(&self.own_user, |person| person.user_id.as_str())
    }

    pub(crate) fn person_name(&self, user: &str) -> &str {
        self.people
            .iter()
            .find(|p| p.user_id == user)
            .map_or("unknown", |p| p.name.as_str())
    }

    pub(crate) fn selected_terminals(&self) -> &[TerminalInfo] {
        self.terminals.get(self.user()).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn focused(&self) -> Option<&str> {
        self.viewer
            .as_ref()
            .map(|(_, pane)| pane.as_str())
            .or_else(|| {
                self.selected_terminals()
                    .get(self.focus)
                    .map(|t| t.pane.as_str())
            })
    }

    pub(crate) fn pane_cursor(&self, pane: &str) -> Option<Cursor> {
        let user = self.viewer.as_ref().map_or(self.user(), |(user, _)| user);
        self.frames
            .get(&(user.into(), pane.into()))
            .map(|frame| frame.cursor)
    }

    pub(crate) fn note_people(&mut self, people: &[Person]) {
        let selected_user = self.user().to_owned();
        self.people = people.to_vec();
        self.people.sort_by_key(|p| p.user_id != self.own_user);
        if let Some(person) = self.people.iter().find(|p| p.user_id == self.own_user) {
            self.own_name.clone_from(&person.name);
        }
        self.selected = self
            .people
            .iter()
            .position(|p| p.user_id == selected_user)
            .unwrap_or(0);
    }

    pub(crate) fn select_person(&mut self, index: usize) {
        self.selected = index.min(self.people.len().saturating_sub(1));
        self.focus = 0;
        self.grid_scroll = 0;
        self.notice.clear();
    }

    pub(crate) fn matches(&self) -> Vec<usize> {
        let query = self.search.to_lowercase();
        self.people
            .iter()
            .enumerate()
            .filter(|(_, person)| {
                person.name.to_lowercase().contains(&query)
                    || (person.user_id == self.own_user && "you".contains(&query))
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub(crate) fn open_focused(&mut self) {
        if let Some(terminal) = self.selected_terminals().get(self.focus) {
            self.viewer = Some((self.user().into(), terminal.pane.clone()));
        }
    }

    pub(crate) fn replace_tree(&mut self, tree: Tree) {
        self.tree = tree;
        let Some(existing) = &self.pending_new else {
            return;
        };
        let terminal = self
            .tree
            .workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .flat_map(|t| &t.panes)
            .find(|p| !existing.contains(&p.id))
            .map(|p| p.id.clone());
        if let Some(pane) = terminal {
            self.selected = self
                .people
                .iter()
                .position(|p| p.user_id == self.own_user)
                .unwrap_or(0);
            self.viewer = Some((self.own_user.clone(), pane));
            self.pending_new = None;
        }
    }

    pub(crate) fn location(&self, pane: &str) -> Option<(String, String)> {
        self.tree.workspaces.iter().find_map(|w| {
            w.tabs
                .iter()
                .find(|t| t.panes.iter().any(|p| p.id == pane))
                .map(|t| (w.id.clone(), t.id.clone()))
        })
    }
}

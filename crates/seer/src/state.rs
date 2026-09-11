use crate::viewer::Viewer;
use ratatui::layout::{Rect, Size};
use seer_core::proto::{ClientMsg, Person, PersonState, TerminalInfo};
use seer_core::{Cursor, TerminalFrame, Tree};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Tile {
    pub(crate) index: usize,
    pub(crate) area: Rect,
    pub(crate) content: Rect,
}

pub(crate) struct ClientState {
    pub(crate) tree: Tree,
    pub(crate) own_user: String,
    pub(crate) own_name: String,
    pub(crate) server: String,
    pub(crate) people: Vec<Person>,
    pub(crate) selected: usize,
    pub(crate) focus: usize,
    pub(crate) chrome: crate::render::Chrome,
    pub(crate) terminals: HashMap<String, Vec<TerminalInfo>>,
    pub(crate) frames: HashMap<(String, String), TerminalFrame>,
    pub(crate) viewer: Option<Viewer>,
    pub(crate) selection: Option<crate::input::Selection>,
    pub(crate) program_pressed: bool,
    pub(crate) menu: Option<crate::person_menu::PersonMenu>,
    pub(crate) can_type_here: BTreeSet<String>,
    pub(crate) you_may_type_into: BTreeSet<String>,
    pub(crate) people_areas: Vec<(usize, Rect)>,
    pub(crate) box_areas: Vec<Tile>,
    pub(crate) picker_scroll: usize,
    pub(crate) grid_scroll: usize,
    pub(crate) grid_columns: usize,
    pub(crate) grid_rows: usize,
    pub(crate) invite: Option<String>,
    pub(crate) invite_pending: bool,
    pub(crate) notice: String,
    pub(crate) room_was_lost: bool,
    pub(crate) watches: BTreeMap<(String, String), Size>,
    pub(crate) pending_new: Option<BTreeSet<String>>,
}

impl ClientState {
    pub(crate) fn chrome_owns_input(&self) -> bool {
        self.chrome.panel.is_some() || self.menu.is_some() || self.chrome.context.is_some()
    }
    pub(crate) fn new(tree: Tree, own_user: String) -> Self {
        let own = Person {
            user_id: own_user.clone(),
            name: own_user.clone(),
            online: true,
            idle_secs: 0,
            attached_clients: 1,
            peekable: true,
            host: false,
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
            chrome: crate::render::Chrome::default(),
            terminals: HashMap::new(),
            frames: HashMap::new(),
            viewer: None,
            selection: None,
            program_pressed: false,
            menu: None,
            can_type_here: BTreeSet::new(),
            you_may_type_into: BTreeSet::new(),
            people_areas: Vec::new(),
            box_areas: Vec::new(),
            picker_scroll: 0,
            grid_scroll: 0,
            grid_columns: 1,
            grid_rows: 0,
            invite: None,
            invite_pending: false,
            notice: String::new(),
            room_was_lost: false,
            watches: BTreeMap::new(),
            pending_new: None,
        }
    }

    pub(crate) fn set_notice(&mut self, notice: impl Into<String>) {
        self.notice = notice.into();
        self.chrome.notice_since = None;
    }

    pub(crate) fn user(&self) -> &str {
        self.people
            .get(self.selected)
            .map_or(&self.own_user, |person| person.user_id.as_str())
    }

    pub(crate) fn display_name(&self, user: &str) -> &str {
        if user == self.own_user {
            return &self.own_name;
        }
        self.person_name(user)
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
            .map(|viewer| viewer.pane.as_str())
            .or_else(|| {
                self.selected_terminals()
                    .get(self.focus)
                    .map(|t| t.pane.as_str())
            })
    }

    pub(crate) fn pane_cursor(&self, pane: &str) -> Option<Cursor> {
        let user = self
            .viewer
            .as_ref()
            .map_or(self.user(), |viewer| viewer.user.as_str());
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
        self.viewer = None;
        self.selection = None;
        self.chrome.grid_focus = false;
        self.grid_scroll = 0;
        self.notice.clear();
    }

    pub(crate) fn open_focused(&mut self) {
        self.selection = None;
        self.chrome.grid_focus = true;
        if let Some(terminal) = self.selected_terminals().get(self.focus) {
            self.viewer = Some(Viewer::new(self.user().into(), terminal.pane.clone()));
        }
    }

    pub(crate) fn select_tab(&mut self, index: usize) {
        if index >= self.selected_terminals().len() {
            return;
        }
        self.focus = index;
        self.chrome.grid_focus = true;
        if self.viewer.is_some() {
            self.open_focused();
        }
        if !self.box_areas.iter().any(|tile| tile.index == index) {
            self.grid_scroll = index / self.grid_columns.max(1);
        }
    }

    pub(crate) fn request_close(&self, stream: &mut crate::routes::Routes) -> io::Result<()> {
        if self.user() != self.own_user {
            return Ok(());
        }
        let Some(pane) = self.focused() else {
            return Ok(());
        };
        let Some((workspace, tab)) = self.location(pane) else {
            return Ok(());
        };
        crate::tui::send(
            stream,
            &ClientMsg::ClosePane {
                workspace,
                tab,
                pane: pane.to_owned(),
            },
        )
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
            self.focus = self
                .terminals
                .get(&self.own_user)
                .and_then(|list| list.iter().position(|t| t.pane == pane))
                .unwrap_or(0);
            self.viewer = Some(Viewer::new(self.own_user.clone(), pane));
            self.pending_new = None;
        }
    }

    pub(crate) fn note_frame(&mut self, user: String, pane: String, frame: TerminalFrame) {
        #[cfg(debug_assertions)]
        seer_core::debug_log::transition(
            &format!("frame user={user} pane={pane}"),
            seer_core::debug_log::frame_summary(&frame),
        );
        self.frames.insert((user, pane), frame);
    }

    pub(crate) fn may_type(&self, user: &str) -> bool {
        user == self.own_user || self.you_may_type_into.contains(user)
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

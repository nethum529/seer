use crate::{state::ClientState, tui::send};
use seer_core::proto::ClientMsg;
use std::io;

pub(crate) fn sync_resize(
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<()> {
    let Some(viewer) = &mut state.viewer else {
        return Ok(());
    };
    let size = viewer.area.as_size();
    if viewer.user != state.own_user || viewer.area.is_empty() || viewer.sent_size == Some(size) {
        return Ok(());
    }
    let pane = viewer.pane.clone();
    let Some((workspace, tab)) = state.location(&pane) else {
        return Ok(());
    };
    if let Some(viewer) = state.viewer.as_mut() {
        viewer.sent_size = Some(size);
    }
    send(
        stream,
        &ClientMsg::Resize {
            workspace,
            tab,
            cols: size.width,
            rows: size.height,
        },
    )
}

pub(crate) fn sync_watches(
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<()> {
    let visible = if let Some(viewer) = &state.viewer {
        vec![(viewer.target(), viewer.area)]
    } else {
        state
            .box_areas
            .iter()
            .filter_map(|tile| {
                state.selected_terminals().get(tile.index).map(|terminal| {
                    (
                        (state.user().to_owned(), terminal.pane.clone()),
                        tile.content,
                    )
                })
            })
            .collect()
    };
    let viewer = state.viewer.is_some();
    let wanted: std::collections::BTreeMap<_, _> = visible
        .into_iter()
        .filter_map(|(target, area)| {
            (!area.is_empty()).then_some((target, (area.as_size(), viewer)))
        })
        .collect();
    let gone: Vec<(String, String)> = state
        .watches
        .keys()
        .filter(|target| !wanted.contains_key(*target))
        .cloned()
        .collect();
    for key in gone {
        send(
            stream,
            &ClientMsg::Unwatch {
                user: key.0.clone(),
                pane: key.1.clone(),
            },
        )?;
        state.forget_sync(&key);
    }
    for ((user, pane), watch) in &wanted {
        if state.watches.get(&(user.clone(), pane.clone())) != Some(watch) {
            let (size, viewer) = *watch;
            send(
                stream,
                &ClientMsg::Watch {
                    user: user.clone(),
                    pane: pane.clone(),
                    cols: size.width,
                    rows: size.height,
                    viewer,
                },
            )?;
        }
    }
    state.watches = wanted;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render;
    use ratatui::{Terminal, backend::TestBackend};
    use seer_core::Tree;
    use seer_core::proto::{TerminalInfo, codec};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    #[test]
    fn watches_and_resize_follow_content_rects() {
        let mut tree = Tree::new();
        let workspace = tree.create_workspace("main").expect("workspace must open");
        let size = seer_core::PaneSize { cols: 80, rows: 24 };
        let panes: Vec<String> = ["one", "two"]
            .map(|title| {
                tree.create_tab(&workspace.id, title, size)
                    .expect("tab must open")
                    .panes[0]
                    .id
                    .clone()
            })
            .into();
        let mut state = ClientState::new(tree, "alice".into());
        state.terminals.insert(
            "alice".into(),
            panes
                .iter()
                .map(|pane| TerminalInfo {
                    pane: pane.clone(),
                    name: "shell".into(),
                    state: "idle".into(),
                    cols: 80,
                    rows: 24,
                    last_typist: None,
                })
                .collect(),
        );
        let (local, mut peer) = UnixStream::pair().expect("streams must open");
        peer.set_read_timeout(Some(Duration::from_millis(50)))
            .expect("timeout must apply");
        let mut stream =
            crate::routes::Routes::new(seer_net::Socket::from(local), None, "alice".to_owned());
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).expect("backend must open");
        terminal
            .draw(|frame| render::draw(frame, &mut state))
            .expect("screen must draw");
        sync_watches(&mut stream, &mut state).expect("watches must send");
        assert_grid_watches(&mut peer, &state);
        sync_watches(&mut stream, &mut state).expect("unchanged watches must sync");
        let mut byte = [0];
        assert!(std::io::Read::read(&mut peer, &mut byte).is_err());
        state.open_focused();
        terminal
            .draw(|frame| render::draw(frame, &mut state))
            .expect("viewer must draw");
        sync_watches(&mut stream, &mut state).expect("viewer watch must send");
        sync_resize(&mut stream, &mut state).expect("resize must send");
        assert_eq!(
            codec::decode::<_, ClientMsg>(&mut peer).expect("unwatch must arrive"),
            ClientMsg::Unwatch {
                user: "alice".into(),
                pane: panes[1].clone()
            }
        );
        assert_viewer_watch(&mut peer, &state);
        assert_viewer_resize(&mut peer, &state);
        terminal.backend_mut().resize(100, 30);
        terminal
            .draw(|frame| render::draw(frame, &mut state))
            .expect("resized viewer must draw");
        sync_watches(&mut stream, &mut state).expect("resized watch must send");
        sync_resize(&mut stream, &mut state).expect("resize must send");
        assert_viewer_watch(&mut peer, &state);
        assert_viewer_resize(&mut peer, &state);
        let viewer_area = state.viewer.as_ref().expect("viewer must exist").area;
        assert_eq!(viewer_area, ratatui::layout::Rect::new(0, 0, 100, 30));
        for panel in [
            Some(crate::panels::Panel::Picker),
            Some(crate::panels::Panel::Session),
            None,
        ] {
            state.chrome.panel = panel;
            terminal
                .draw(|frame| render::draw(frame, &mut state))
                .expect("overlay must draw");
            sync_watches(&mut stream, &mut state).expect("watches must sync");
            sync_resize(&mut stream, &mut state).expect("resize must sync");
            assert!(
                std::io::Read::read(&mut peer, &mut byte).is_err(),
                "overlay must not resize the terminal"
            );
            assert_eq!(
                state.viewer.as_ref().expect("viewer must exist").area,
                viewer_area
            );
        }
        terminal.backend_mut().resize(50, 20);
        terminal
            .draw(|frame| render::draw(frame, &mut state))
            .expect("narrow window must draw");
        sync_watches(&mut stream, &mut state).expect("narrow watch must send");
        sync_resize(&mut stream, &mut state).expect("narrow resize must send");
        assert_eq!(
            state.viewer.as_ref().expect("viewer must exist").area,
            ratatui::layout::Rect::new(0, 0, 50, 20),
            "a narrow window must report the whole window"
        );
        assert_viewer_watch(&mut peer, &state);
        assert_viewer_resize(&mut peer, &state);
        terminal.backend_mut().resize(100, 30);
        terminal
            .draw(|frame| render::draw(frame, &mut state))
            .expect("restored size must draw");
        sync_watches(&mut stream, &mut state).expect("restored watch must send");
        sync_resize(&mut stream, &mut state).expect("restored resize must send");
        assert_viewer_watch(&mut peer, &state);
        assert_viewer_resize(&mut peer, &state);
        state.viewer = None;
        state
            .terminals
            .get_mut("alice")
            .expect("terminals must exist")
            .truncate(1);
        terminal
            .draw(|frame| render::draw(frame, &mut state))
            .expect("grid must draw");
        sync_watches(&mut stream, &mut state).expect("grid watches must sync");
        assert_grid_watches(&mut peer, &state);
        let tile = &state.box_areas[0];
        assert_eq!(tile.content, viewer_area);
        assert_eq!(tile.content.right(), 100);
        assert_eq!(tile.content.bottom(), 30);
        assert_eq!(tile.content.x, 0);
    }

    #[test]
    fn resize_retries_after_the_pane_location_arrives() {
        let mut tree = Tree::new();
        let workspace = tree.create_workspace("main").expect("workspace must open");
        let tab = tree
            .create_tab(
                &workspace.id,
                "one",
                seer_core::PaneSize { cols: 80, rows: 24 },
            )
            .expect("tab must open");
        let pane = tab.panes[0].id.clone();
        let mut state = ClientState::new(Tree::new(), "alice".into());
        let mut viewer = crate::viewer::Viewer::new("alice".into(), pane);
        viewer.area = ratatui::layout::Rect::new(0, 0, 190, 50);
        state.viewer = Some(viewer);
        let (local, mut peer) = UnixStream::pair().expect("streams must open");
        peer.set_read_timeout(Some(Duration::from_millis(50)))
            .expect("timeout must apply");
        let mut stream =
            crate::routes::Routes::new(seer_net::Socket::from(local), None, "alice".to_owned());

        sync_resize(&mut stream, &mut state).expect("resize must sync");
        let mut byte = [0];
        assert!(
            std::io::Read::read(&mut peer, &mut byte).is_err(),
            "a pane with no location must not resize"
        );

        state.replace_tree(tree);
        sync_resize(&mut stream, &mut state).expect("resize must retry");
        assert_viewer_resize(&mut peer, &state);
    }

    fn assert_grid_watches(peer: &mut UnixStream, state: &ClientState) {
        for tile in &state.box_areas {
            assert_eq!(
                codec::decode::<_, ClientMsg>(peer).expect("grid watch must arrive"),
                ClientMsg::Watch {
                    user: "alice".into(),
                    pane: state.selected_terminals()[tile.index].pane.clone(),
                    cols: tile.content.width,
                    rows: tile.content.height,
                    viewer: false,
                }
            );
        }
    }

    fn assert_viewer_resize(peer: &mut UnixStream, state: &ClientState) {
        let viewer = state.viewer.as_ref().expect("viewer must exist");
        let (workspace, tab) = state.location(&viewer.pane).unwrap_or_default();
        assert_eq!(
            codec::decode::<_, ClientMsg>(peer).expect("resize must arrive"),
            ClientMsg::Resize {
                workspace,
                tab,
                cols: viewer.area.width,
                rows: viewer.area.height,
            }
        );
    }

    fn assert_viewer_watch(peer: &mut UnixStream, state: &ClientState) {
        let viewer = state.viewer.as_ref().expect("viewer must exist");
        assert_eq!(
            codec::decode::<_, ClientMsg>(peer).expect("viewer watch must arrive"),
            ClientMsg::Watch {
                user: viewer.user.clone(),
                pane: viewer.pane.clone(),
                cols: viewer.area.width,
                rows: viewer.area.height,
                viewer: true,
            }
        );
    }
}

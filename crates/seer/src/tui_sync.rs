use crate::{state::ClientState, tui::send};
use seer_core::proto::ClientMsg;
use seer_net::Stream;
use std::io;

pub(crate) fn sync_resize(stream: &mut impl Stream, state: &mut ClientState) -> io::Result<()> {
    let Some(viewer) = &mut state.viewer else {
        return Ok(());
    };
    let size = viewer.area.as_size();
    if viewer.user != state.own_user || viewer.area.is_empty() || viewer.sent_size == Some(size) {
        return Ok(());
    }
    viewer.sent_size = Some(size);
    let pane = viewer.pane.clone();
    if let Some((workspace, tab)) = state.location(&pane) {
        send(
            stream,
            &ClientMsg::Resize {
                workspace,
                tab,
                cols: size.width,
                rows: size.height,
            },
        )?;
    }
    Ok(())
}

pub(crate) fn sync_watches(stream: &mut impl Stream, state: &mut ClientState) -> io::Result<()> {
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
    let wanted: std::collections::BTreeMap<_, _> = visible
        .into_iter()
        .filter_map(|(target, area)| (!area.is_empty()).then_some((target, area.as_size())))
        .collect();
    for (user, pane) in state
        .watches
        .keys()
        .filter(|target| !wanted.contains_key(*target))
    {
        send(
            stream,
            &ClientMsg::Unwatch {
                user: user.clone(),
                pane: pane.clone(),
            },
        )?;
    }
    for ((user, pane), size) in &wanted {
        if state.watches.get(&(user.clone(), pane.clone())) != Some(size) {
            send(
                stream,
                &ClientMsg::Watch {
                    user: user.clone(),
                    pane: pane.clone(),
                    cols: size.width,
                    rows: size.height,
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
        let (mut stream, mut peer) = UnixStream::pair().expect("streams must open");
        peer.set_read_timeout(Some(Duration::from_millis(50)))
            .expect("timeout must apply");
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
            Some(crate::panels::Panel::People),
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
        assert!(std::io::Read::read(&mut peer, &mut byte).is_err());
        let tile = &state.box_areas[0];
        assert_eq!(tile.content, viewer_area);
        assert_eq!(tile.content.right(), 100);
        assert_eq!(tile.content.bottom(), 30);
        assert_eq!(tile.content.x, 0);
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
            }
        );
    }
}

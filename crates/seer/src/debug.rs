use crate::state::ClientState;
use ratatui::layout::Rect;

pub(crate) fn render_summary(state: &ClientState, area: Rect) -> String {
    let view = match &state.viewer {
        Some(viewer) => format!(
            "viewer user={} pane={} area={}x{} offset={}",
            viewer.user, viewer.pane, viewer.area.width, viewer.area.height, viewer.offset
        ),
        None => format!(
            "grid columns={} rows={} scroll={} tiles={}",
            state.grid_columns,
            state.grid_rows,
            state.grid_scroll,
            state.box_areas.len()
        ),
    };
    format!(
        "window={}x{} selected={} focus={} {view} panel={:?} menu={} context={} grid_focus={} selection={}",
        area.width,
        area.height,
        state.user(),
        state.focus,
        state.chrome.panel,
        state.menu.is_some(),
        state.chrome.context.is_some(),
        state.chrome.grid_focus,
        state.selection.is_some()
    )
}

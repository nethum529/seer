use super::*;

impl PaneGrid {
    pub(crate) fn agent_screen(&self) -> String {
        let grid = self.terminal.grid();
        let lines: Vec<_> = (0..grid.screen_lines())
            .rev()
            .take(256)
            .filter_map(|row| {
                let line = Line(row as i32);
                let text: String = (0..grid.columns().min(512))
                    .map(|column| &grid[line][Column(column)])
                    .filter(|cell| {
                        !cell
                            .flags
                            .intersects(Flags::HIDDEN | Flags::WIDE_CHAR_SPACER)
                    })
                    .map(|cell| cell.c)
                    .collect();
                let text = text.trim_end();
                (!text.is_empty()).then(|| text.to_owned())
            })
            .take(32)
            .collect();
        lines.into_iter().rev().collect::<Vec<_>>().join("\n")
    }

    pub(crate) fn title(&self) -> String {
        lock_mutex(&self.replies.title).clone()
    }
}

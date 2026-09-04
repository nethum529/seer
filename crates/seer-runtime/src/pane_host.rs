use crate::{PaneGrid, PtySession};
use portable_pty::CommandBuilder;
use seer_core::{TerminalFrame, TerminalInput};
use std::io;

pub struct PaneHost {
    session: PtySession,
    grid: PaneGrid,
}

impl PaneHost {
    pub fn start(command: CommandBuilder, cols: u16, rows: u16) -> io::Result<Self> {
        Ok(Self {
            session: PtySession::start(command, cols, rows)?,
            grid: PaneGrid::new(cols, rows),
        })
    }

    pub fn poll(&mut self) -> bool {
        let output = self.session.drain_output();
        let changed = self.grid.feed(&output);
        let replies = self.grid.take_replies();
        if !replies.is_empty()
            && let Err(error) = self.session.write_input(&replies)
        {
            eprintln!("pane reply write failed: {error}");
        }
        changed
    }

    pub fn handle_input(&mut self, input: &TerminalInput) -> io::Result<bool> {
        if let Some(bytes) = self.grid.handle_input(input)? {
            self.session.write_input(&bytes)?;
        }
        Ok(self.grid.take_input_changed())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {
        self.session.resize(cols, rows)?;
        self.grid.resize(cols, rows);
        Ok(())
    }

    pub fn frame(&self) -> TerminalFrame {
        self.grid.snapshot()
    }

    pub fn foreground(&self) -> String {
        self.session.foreground_name()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.session.kill()
    }
}

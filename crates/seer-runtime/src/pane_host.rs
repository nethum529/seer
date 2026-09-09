use crate::{PaneGrid, PtySession};
use portable_pty::CommandBuilder;
use seer_core::{TerminalFrame, TerminalInput};
use std::io;
use std::time::{Duration, Instant};

pub struct PaneHost {
    session: PtySession,
    grid: PaneGrid,
    pub(crate) owner_size: seer_core::PaneSize,
    last_typist: Option<(String, Instant)>,
}

impl PaneHost {
    pub fn start(command: CommandBuilder, cols: u16, rows: u16) -> io::Result<Self> {
        Ok(Self {
            session: PtySession::start(command, cols, rows)?,
            grid: PaneGrid::new(cols, rows),
            owner_size: seer_core::PaneSize { cols, rows },
            last_typist: None,
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

    pub(crate) fn write_granted(&mut self, bytes: &[u8], sender: String) -> io::Result<()> {
        self.session.write_input(bytes)?;
        if !bytes.is_empty() {
            self.last_typist = Some((sender, Instant::now()));
        }
        Ok(())
    }

    pub(crate) fn last_typist(&self) -> Option<String> {
        self.last_typist
            .as_ref()
            .filter(|(_, at)| at.elapsed() < Duration::from_secs(5))
            .map(|(name, _)| name.clone())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {
        self.resize_visible(cols, rows)?;
        self.owner_size = seer_core::PaneSize { cols, rows };
        Ok(())
    }

    pub(crate) fn remember_owner_size(&mut self, size: seer_core::PaneSize) {
        self.owner_size = size;
    }

    pub(crate) fn resize_visible(&mut self, cols: u16, rows: u16) -> io::Result<()> {
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

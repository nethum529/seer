use crate::outbox::Outbox;
use crate::pane_output::{self, Output, PaneOutput};
use crate::{PaneGrid, PtySession};
use portable_pty::CommandBuilder;
use seer_core::{TerminalFrame, TerminalInput};
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct PaneHost {
    session: PtySession,
    output: Output,
    pub(crate) owner_size: seer_core::PaneSize,
    last_typist: Option<(String, Instant)>,
    program: String,
}

impl PaneHost {
    pub fn start(command: CommandBuilder, cols: u16, rows: u16) -> io::Result<Self> {
        let program = command
            .get_argv()
            .first()
            .and_then(|program| Path::new(program).file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let shared = PaneOutput::shared(PaneGrid::new(cols, rows));
        let reader = pane_output::reader(Arc::clone(&shared));
        let session = PtySession::start(command, cols, rows, reader)?;
        Ok(Self {
            output: Output::new(shared, session.answers()),
            session,
            owner_size: seer_core::PaneSize { cols, rows },
            last_typist: None,
            program,
        })
    }

    pub fn poll(&mut self) -> bool {
        self.output.answer(PaneOutput::poll)
    }

    /// Gives the output, which takes the Parsed messages of the sink.
    pub(crate) fn attach_raw(&self, sink: Arc<Outbox>) -> Output {
        self.output.answer(|pane| ((), pane.attach(sink)));
        self.output.clone()
    }

    pub(crate) fn detach_raw(&self, sink: &Arc<Outbox>) {
        self.output.answer(|pane| ((), pane.detach(sink)));
    }

    pub fn handle_input(&mut self, input: &TerminalInput) -> io::Result<bool> {
        let (bytes, changed) = {
            let mut output = self.output.lock();
            let bytes = output.grid.handle_input(input)?;
            (bytes, output.grid.take_input_changed())
        };
        if let Some(bytes) = bytes {
            self.session.write_input(&bytes)?;
        }
        Ok(changed)
    }

    pub(crate) fn write_granted(&mut self, bytes: &[u8], sender: String) -> io::Result<()> {
        self.session.write_input(bytes)?;
        if !bytes.is_empty() {
            self.last_typist = Some((sender, Instant::now()));
        }
        Ok(())
    }

    pub(crate) fn write_granted_mouse(
        &mut self,
        mouse: seer_core::MouseInput,
        sender: String,
    ) -> io::Result<()> {
        let input = TerminalInput::new(seer_core::InputEvent::Mouse(mouse));
        let bytes = {
            let mut output = self.output.lock();
            if output.grid.modes().mouse_tracking == seer_core::MouseTracking::None {
                return Ok(());
            }
            output.grid.handle_input(&input)?
        };
        if let Some(bytes) = bytes {
            self.write_granted(&bytes, sender)?;
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
        let mut output = self.output.lock();
        self.session.resize(cols, rows)?;
        output.resize(cols, rows);
        Ok(())
    }

    pub fn frame(&self) -> TerminalFrame {
        self.output.lock().grid.snapshot()
    }

    pub(crate) fn view(&self, size: seer_core::PaneSize) -> Option<TerminalFrame> {
        self.output.lock().grid.view(size.cols, size.rows)
    }

    pub(crate) fn alt_screen(&self) -> bool {
        self.output.lock().grid.modes().alt_screen
    }

    pub fn foreground(&self) -> String {
        self.session.foreground_name()
    }

    pub(crate) fn started_program(&self) -> &str {
        &self.program
    }

    pub(crate) fn has_exited(&mut self) -> io::Result<bool> {
        self.session.has_exited()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.session.kill()
    }
}

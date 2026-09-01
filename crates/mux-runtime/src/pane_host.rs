use crate::{Cell, PaneGrid, PtySession};
use portable_pty::CommandBuilder;
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

    pub fn poll(&mut self) -> usize {
        let output = self.session.drain_output();
        let count = output.len();
        self.grid.feed(&output);
        count
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.session.write_input(bytes)
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {
        self.session.resize(cols, rows)?;
        self.grid.resize(cols, rows);
        Ok(())
    }

    pub fn cells(&self) -> Vec<Vec<Cell>> {
        self.grid.snapshot()
    }

    pub fn is_alive(&mut self) -> io::Result<bool> {
        self.session.is_alive()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.session.kill()
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::thread;
    use std::time::{Duration, Instant};

    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
    const POLL_INTERVAL: Duration = Duration::from_millis(10);

    #[test]
    fn polls_process_output_into_cells() {
        let mut command = CommandBuilder::new("sh");
        command.args(["-c", "printf hi"]);
        let mut host = PaneHost::start(command, 10, 2).expect("pane host must start");

        let fed = wait_for_text(&mut host, "hi").expect("output must render");
        assert!(fed >= 2);
        assert_eq!(host.cells()[0][0].character, 'h');
        assert_eq!(host.cells()[0][1].character, 'i');
    }

    #[test]
    fn writes_input_into_process_and_renders_output() {
        let command = CommandBuilder::new("cat");
        let mut host = PaneHost::start(command, 20, 4).expect("pane host must start");

        host.write_input(b"hello\n").expect("input must be written");

        assert!(wait_for_text(&mut host, "hello").is_some());
        host.kill().expect("pane process must stop");
        assert!(wait_for_exit(&mut host));
    }

    #[test]
    fn resizes_session_and_grid() {
        let command = CommandBuilder::new("cat");
        let mut host = PaneHost::start(command, 2, 2).expect("pane host must start");

        host.resize(4, 3).expect("pane host must resize");

        let cells = host.cells();
        assert_eq!(cells.len(), 3);
        assert!(cells.iter().all(|row| row.len() == 4));
        assert!(host.is_alive().expect("process state must be readable"));
        host.kill().expect("pane process must stop");
    }

    fn wait_for_text(host: &mut PaneHost, expected: &str) -> Option<usize> {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        let mut fed = 0;
        while Instant::now() < deadline {
            fed += host.poll();
            if visible_text(host).contains(expected) {
                return Some(fed);
            }
            thread::sleep(POLL_INTERVAL);
        }
        None
    }

    fn wait_for_exit(host: &mut PaneHost) -> bool {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while Instant::now() < deadline {
            match host.is_alive() {
                Ok(false) => return true,
                Ok(true) => thread::sleep(POLL_INTERVAL),
                Err(_) => return false,
            }
        }
        false
    }

    fn visible_text(host: &PaneHost) -> String {
        host.cells()
            .into_iter()
            .flatten()
            .map(|cell| cell.character)
            .collect()
    }
}

use std::io::{self, Stdout};

use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::style::Print;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use seer_core::CursorShape;

use crate::state::ClientState;

// Mode 1007 makes the host send wheel events to the program instead of
// scrolling its own scrollback while the alternate screen is up. Without it a
// host with alternate scroll off shows the lines from before Seer started.
const ALTERNATE_SCROLL_ON: &str = "\x1b[?1007h";
const ALTERNATE_SCROLL_OFF: &str = "\x1b[?1007l";

pub(crate) struct TerminalSession {
    pub(crate) terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    pub(crate) fn start() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(
            stdout,
            EnterAlternateScreen,
            Print(ALTERNATE_SCROLL_ON),
            EnableMouseCapture,
            EnableBracketedPaste,
            EnableFocusChange,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES,
            ),
            SetTitle("Seer")
        ) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                restore_terminal();
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        let _ = execute!(io::stdout(), SetCursorStyle::DefaultUserShape);
        restore_terminal();
        let _ = disable_raw_mode();
    }
}

fn restore_terminal() {
    let _ = execute!(
        io::stdout(),
        PopKeyboardEnhancementFlags,
        DisableFocusChange,
        DisableBracketedPaste,
        DisableMouseCapture,
        Print(ALTERNATE_SCROLL_OFF),
        LeaveAlternateScreen
    );
}

pub(crate) fn set_cursor_style(state: &ClientState) -> io::Result<()> {
    let style = state
        .focused()
        .and_then(|pane| state.pane_cursor(pane))
        .map_or(SetCursorStyle::DefaultUserShape, cursor_style);
    execute!(io::stdout(), style)
}

pub(crate) fn ignore_setup_disconnect(error: io::Error) -> io::Result<()> {
    match error.kind() {
        io::ErrorKind::BrokenPipe
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::ConnectionReset => Ok(()),
        _ => Err(error),
    }
}

fn cursor_style(cursor: seer_core::Cursor) -> SetCursorStyle {
    match (cursor.shape, cursor.blinking) {
        (CursorShape::Underline, true) => SetCursorStyle::BlinkingUnderScore,
        (CursorShape::Underline, false) => SetCursorStyle::SteadyUnderScore,
        (CursorShape::Beam, true) => SetCursorStyle::BlinkingBar,
        (CursorShape::Beam, false) => SetCursorStyle::SteadyBar,
        (CursorShape::Block | CursorShape::HollowBlock, true) => SetCursorStyle::BlinkingBlock,
        (CursorShape::Block | CursorShape::HollowBlock, false) => SetCursorStyle::SteadyBlock,
    }
}

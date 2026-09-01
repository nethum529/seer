use std::cell::Cell as ModeCell;
use std::io::{self, Stdout};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use mux_core::SplitDirection::{Down, Right};
use mux_core::proto::{ClientMsg, ServerMsg, codec};
use mux_core::{Cell, Color, Tree};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

use crate::input::key_to_bytes;
use crate::state::{ClientState, pane_rects};

const EVENT_WAIT: Duration = Duration::from_millis(25);

thread_local! {
    static VIEW_ONLY: ModeCell<bool> = const { ModeCell::new(false) };
}

enum ReaderEvent {
    Message(ServerMsg),
    Failed(io::Error),
}

#[derive(Debug, PartialEq)]
enum LoopControl {
    Continue,
    Exit,
}

pub(crate) fn run(mut stream: TcpStream, tree: Tree) -> io::Result<()> {
    let mut terminal = TerminalSession::start()?;
    let reader = stream.try_clone()?;
    let (receiver, reader_thread) = spawn_reader(reader);
    let loop_result = run_loop(&mut terminal.terminal, &mut stream, &receiver, tree);
    drop(terminal);
    let _ = stream.shutdown(Shutdown::Both);
    join_reader(reader_thread)?;
    loop_result
}

fn spawn_reader(mut stream: TcpStream) -> (Receiver<ReaderEvent>, JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        loop {
            match codec::decode(&mut stream) {
                Ok(message) => {
                    if sender.send(ReaderEvent::Message(message)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = sender.send(ReaderEvent::Failed(error));
                    break;
                }
            }
        }
    });
    (receiver, handle)
}

fn join_reader(reader: JoinHandle<()>) -> io::Result<()> {
    reader
        .join()
        .map_err(|_| io::Error::other("socket reader thread panicked"))
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    stream: &mut TcpStream,
    receiver: &Receiver<ReaderEvent>,
    tree: Tree,
) -> io::Result<()> {
    let mut state = ClientState::new(tree);
    let mut command_pending = false;
    let mut dirty = true;

    loop {
        if receive_messages(receiver, &mut state, &mut dirty)? == LoopControl::Exit {
            return Ok(());
        }
        if dirty {
            terminal.draw(|frame| draw(frame, &state))?;
            dirty = false;
        }
        if event::poll(EVENT_WAIT)? {
            let event = event::read()?;
            if handle_event(event, stream, &mut state, &mut command_pending)? == LoopControl::Exit {
                return Ok(());
            }
            dirty = true;
        }
    }
}

fn receive_messages(
    receiver: &Receiver<ReaderEvent>,
    state: &mut ClientState,
    dirty: &mut bool,
) -> io::Result<LoopControl> {
    loop {
        match receiver.try_recv() {
            Ok(ReaderEvent::Message(message)) => {
                if apply_server_message(message, state)? == LoopControl::Exit {
                    return Ok(LoopControl::Exit);
                }
                *dirty = true;
            }
            Ok(ReaderEvent::Failed(error)) => return Err(error),
            Err(TryRecvError::Empty) => return Ok(LoopControl::Continue),
            Err(TryRecvError::Disconnected) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "server connection closed",
                ));
            }
        }
    }
}

fn apply_server_message(message: ServerMsg, state: &mut ClientState) -> io::Result<LoopControl> {
    match message {
        ServerMsg::Tree { tree } => state.replace_tree(tree),
        ServerMsg::Cells { pane, rows } => state.apply_cells(pane, rows),
        ServerMsg::Bye { .. } => return Ok(LoopControl::Exit),
        ServerMsg::Frame { .. } => {}
        ServerMsg::Welcome { .. } | ServerMsg::Refused { .. } => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected handshake message",
            ));
        }
    }
    Ok(LoopControl::Continue)
}

fn handle_event(
    event: Event,
    stream: &mut TcpStream,
    state: &mut ClientState,
    command_pending: &mut bool,
) -> io::Result<LoopControl> {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            handle_key(key, stream, state, command_pending)
        }
        Event::Resize(cols, rows) if !is_view_only() => {
            send(stream, &ClientMsg::Resize { cols, rows })?;
            Ok(LoopControl::Continue)
        }
        _ => Ok(LoopControl::Continue),
    }
}

fn handle_key(
    key: KeyEvent,
    stream: &mut TcpStream,
    state: &mut ClientState,
    command_pending: &mut bool,
) -> io::Result<LoopControl> {
    if is_control_char(key, 'q') {
        send(stream, &ClientMsg::Detach)?;
        return Ok(LoopControl::Exit);
    }
    if is_view_only() {
        return Ok(LoopControl::Continue);
    }
    if is_control_char(key, 'b') {
        *command_pending = true;
        return Ok(LoopControl::Continue);
    }
    if *command_pending {
        *command_pending = false;
        let command = match key.code {
            KeyCode::Char('c') => Some(ClientMsg::CreateTab),
            KeyCode::Char('%') => Some(ClientMsg::SplitPane { direction: Right }),
            KeyCode::Char('"') => Some(ClientMsg::SplitPane { direction: Down }),
            _ => None,
        };
        if let Some(command) = command {
            send(stream, &command)?;
            return Ok(LoopControl::Continue);
        }
        if let KeyCode::Char(number @ '1'..='9') = key.code {
            let index = number.to_digit(10).map_or(0, |value| value as usize);
            if let Some(pane) = state.focus_number(index) {
                send(stream, &ClientMsg::FocusPane { pane })?;
            }
            return Ok(LoopControl::Continue);
        }
    }
    if let (Some(pane), Some(bytes)) = (state.focused(), key_to_bytes(key)) {
        send(
            stream,
            &ClientMsg::Input {
                pane: pane.to_owned(),
                bytes,
            },
        )?;
    }
    Ok(LoopControl::Continue)
}

fn is_control_char(key: KeyEvent, character: char) -> bool {
    key.code == KeyCode::Char(character) && key.modifiers.contains(KeyModifiers::CONTROL)
}

pub(crate) fn set_view_only(view_only: bool) {
    VIEW_ONLY.set(view_only);
}

fn is_view_only() -> bool {
    VIEW_ONLY.get()
}

fn send(stream: &mut TcpStream, message: &ClientMsg) -> io::Result<()> {
    codec::encode(stream, message)
}

fn draw(frame: &mut ratatui::Frame<'_>, state: &ClientState) {
    let area = frame.area();
    let Some(tab) = state.visible_tab() else {
        return;
    };
    for (pane, pane_area) in pane_rects(tab, area) {
        let block = Block::default().borders(Borders::ALL).title(pane.as_str());
        let inner = block.inner(pane_area);
        frame.render_widget(block, pane_area);
        frame.render_widget(PaneCells::new(state.pane_rows(&pane)), inner);
    }
}

struct PaneCells<'a> {
    rows: &'a [Vec<Cell>],
}

impl<'a> PaneCells<'a> {
    fn new(rows: &'a [Vec<Cell>]) -> Self {
        Self { rows }
    }
}

impl Widget for PaneCells<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        for (row_index, row) in self.rows.iter().take(area.height as usize).enumerate() {
            let y = area.y.saturating_add(row_index as u16);
            for (column_index, cell) in row.iter().take(area.width as usize).enumerate() {
                let x = area.x.saturating_add(column_index as u16);
                buffer[(x, y)]
                    .set_char(cell.character)
                    .set_style(cell_style(cell));
            }
        }
    }
}

fn cell_style(cell: &Cell) -> Style {
    let style = Style::default().fg(color(cell.fg));
    if cell.bold {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn color(color: Color) -> ratatui::style::Color {
    match color {
        Color::Default => ratatui::style::Color::Reset,
        Color::Indexed(index) => ratatui::style::Color::Indexed(index),
        Color::Rgb { red, green, blue } => ratatui::style::Color::Rgb(red, green, blue),
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn start() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read};
    use std::net::{TcpListener, TcpStream};
    use std::time::Duration;

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use mux_core::proto::{ClientMsg, codec};
    use mux_core::{PaneSize, SplitDirection, Tree};

    use super::{LoopControl, handle_event, set_view_only};
    use crate::state::ClientState;

    #[test]
    fn view_only_events_send_no_session_changes() {
        let (mut client, mut server) = socket_pair();
        let mut state = state_with_pane();
        let mut command_pending = false;
        set_view_only(true);
        let events = [
            Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            Event::Key(KeyEvent::new(KeyCode::Char('%'), KeyModifiers::SHIFT)),
            Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            Event::Key(KeyEvent::new(KeyCode::Char('"'), KeyModifiers::SHIFT)),
            Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            Event::Key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)),
            Event::Resize(120, 40),
        ];

        for event in events {
            assert_eq!(
                handle_event(event, &mut client, &mut state, &mut command_pending)
                    .expect("event handling must succeed"),
                LoopControl::Continue
            );
        }

        assert_no_message(&mut server);
    }

    #[test]
    fn active_events_send_input_focus_and_resize() {
        let (mut client, mut server) = socket_pair();
        let mut state = state_with_pane();
        let mut command_pending = false;
        set_view_only(false);
        let events = [
            Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            Event::Key(KeyEvent::new(KeyCode::Char('%'), KeyModifiers::SHIFT)),
            Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            Event::Key(KeyEvent::new(KeyCode::Char('"'), KeyModifiers::SHIFT)),
            Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            Event::Key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)),
            Event::Resize(120, 40),
        ];

        for event in events {
            handle_event(event, &mut client, &mut state, &mut command_pending)
                .expect("active event must be handled");
        }

        let expected = [
            ClientMsg::Input {
                pane: "w1:p1".into(),
                bytes: b"a".to_vec(),
            },
            ClientMsg::CreateTab,
            ClientMsg::SplitPane {
                direction: SplitDirection::Right,
            },
            ClientMsg::SplitPane {
                direction: SplitDirection::Down,
            },
            ClientMsg::FocusPane {
                pane: "w1:p1".into(),
            },
            ClientMsg::Resize {
                cols: 120,
                rows: 40,
            },
        ];
        for message in expected {
            assert_eq!(decode(&mut server), message);
        }
    }

    #[test]
    fn release_keys_send_no_messages() {
        let (mut client, mut server) = socket_pair();
        let mut state = state_with_pane();
        let mut command_pending = false;
        set_view_only(false);
        let release = KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );

        assert_eq!(
            handle_event(
                Event::Key(release),
                &mut client,
                &mut state,
                &mut command_pending,
            )
            .expect("release key must be handled"),
            LoopControl::Continue
        );

        assert_no_message(&mut server);
    }

    #[test]
    fn control_q_detaches_in_view_only_mode() {
        let (mut client, mut server) = socket_pair();
        let mut state = state_with_pane();
        let mut command_pending = false;
        set_view_only(true);

        assert_eq!(
            handle_event(
                Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)),
                &mut client,
                &mut state,
                &mut command_pending,
            )
            .expect("detach key must be handled"),
            LoopControl::Exit
        );
        assert_eq!(decode(&mut server), ClientMsg::Detach);
    }

    fn decode(stream: &mut TcpStream) -> ClientMsg {
        codec::decode(stream).expect("client message must decode")
    }

    fn assert_no_message(stream: &mut TcpStream) {
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("read timeout must be set");
        let mut byte = [0];
        let error = stream
            .read_exact(&mut byte)
            .expect_err("event must not send a message");
        assert!(matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ));
    }

    fn socket_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
        let client = TcpStream::connect(
            listener
                .local_addr()
                .expect("listener must have an address"),
        )
        .expect("client must connect");
        let (server, _) = listener.accept().expect("server must accept client");
        server
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("read timeout must be set");
        (client, server)
    }

    fn state_with_pane() -> ClientState {
        let mut tree = Tree::new();
        tree.create_workspace("main")
            .expect("workspace must be created");
        tree.create_tab("w1", "shell", PaneSize { cols: 80, rows: 24 })
            .expect("tab must be created");
        ClientState::new(tree)
    }
}

use std::net::TcpListener;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use gpui::TestAppContext;
use seer::ServerEntry;
use seer_core::proto::{ClientMsg, Person, PersonState, ServerMsg, TerminalInfo, codec};
use seer_core::{Cell, Cursor, TerminalFrame, TerminalModes, Tree};

use super::input_e2e::Runtime;
use super::*;

fn person(user: &str, host: bool) -> Person {
    Person {
        user_id: user.into(),
        name: user.into(),
        host,
        online: true,
        idle_secs: 0,
        attached_clients: 1,
        peekable: true,
        state: PersonState::Active,
        tabs: 1,
        foreground: "shell".into(),
    }
}

fn terminal() -> TerminalInfo {
    TerminalInfo {
        title: Some("Release".into()),
        pane: "remote".into(),
        name: "shell".into(),
        state: "idle".into(),
        cols: 80,
        rows: 24,
        last_typist: None,
        agent_state: None,
        agent_state_since: None,
    }
}

pub(super) fn frame(text: &str) -> ServerMsg {
    ServerMsg::Cells {
        user: "Kyler".into(),
        pane: "remote".into(),
        seq: 1,
        frame: TerminalFrame {
            rows: vec![
                text.chars()
                    .map(|character| Cell {
                        character,
                        fg: seer_core::Color::Default,
                        bg: seer_core::Color::Default,
                        bold: false,
                        italic: false,
                        underline: false,
                        dim: false,
                        inverse: false,
                        hidden: false,
                        strikeout: false,
                    })
                    .collect(),
            ],
            cursor: Cursor::default(),
            modes: TerminalModes::default(),
        },
    }
}

pub(super) struct Room {
    pub(super) server: ServerEntry,
    pub(super) messages: mpsc::Sender<ServerMsg>,
    pub(super) received: Arc<Mutex<Vec<ClientMsg>>>,
}

impl Room {
    pub(super) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server: ServerEntry = serde_json::from_value(serde_json::json!({
            "endpoint": listener.local_addr().unwrap().to_string(), "alias": "E2e",
            "user_id": "e2e", "name": "You", "credential": "e2e", "current": true,
        }))
        .unwrap();
        let (messages, outgoing) = mpsc::channel();
        let received = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&received);
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let _: ClientMsg = codec::decode(&mut socket).unwrap();
            codec::encode(
                &mut socket,
                &ServerMsg::Welcome {
                    user_id: "e2e".into(),
                    name: "You".into(),
                    client_id: "window".into(),
                    tree: Tree::new(),
                },
            )
            .unwrap();
            codec::encode(
                &mut socket,
                &ServerMsg::Grants {
                    can_type_here: Vec::new(),
                    you_may_type_into: Vec::new(),
                },
            )
            .unwrap();
            let mut reader = socket.try_clone().unwrap();
            let writer = Arc::new(Mutex::new(socket));
            let feed = Arc::clone(&writer);
            thread::spawn(move || {
                for message in outgoing {
                    if codec::encode(&mut *feed.lock().unwrap(), &message).is_err() {
                        break;
                    }
                }
            });
            while let Ok(message) = codec::decode::<_, ClientMsg>(&mut reader) {
                let reply = match &message {
                    ClientMsg::ListPeople => Some(ServerMsg::People {
                        people: vec![person("Kyler", true), person("e2e", false)],
                    }),
                    ClientMsg::Terminals { user } if user == "Kyler" => {
                        Some(ServerMsg::Terminals {
                            user: user.clone(),
                            terminals: vec![terminal()],
                        })
                    }
                    ClientMsg::Watch { user, .. } if user == "Kyler" => Some(frame("Remote live")),
                    _ => None,
                };
                log.lock().unwrap().push(message);
                if let Some(reply) = reply {
                    codec::encode(&mut *writer.lock().unwrap(), &reply).unwrap();
                }
            }
        });
        Self {
            server,
            messages,
            received,
        }
    }
}

#[gpui::test]
fn person_menu_watches_read_only_and_returns_to_local_input(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let room = Room::start();
    let (_, cx) = open_view(&runtime, &room, cx);
    pump(cx);
    cx.simulate_click(gpui::point(px(90.), px(62.)), gpui::Modifiers::default());
    cx.simulate_keystrokes("down enter");
    pump(cx);
    assert!(
        room.received
            .lock()
            .unwrap()
            .iter()
            .any(|message| matches!(message, ClientMsg::Watch { user, .. } if user == "Kyler")),
        "picking a person must subscribe to their live terminal"
    );
    cx.simulate_click(gpui::point(px(300.), px(100.)), gpui::Modifiers::default());
    let before = runtime.input();
    cx.simulate_keystrokes("a enter up ctrl-c->\x03");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("blocked".into()));
    cx.simulate_keystrokes("secondary-v");
    pump(cx);
    assert_eq!(
        runtime.input(),
        before,
        "remote keys must not reach the local shell"
    );
    assert!(
        !room
            .received
            .lock()
            .unwrap()
            .iter()
            .any(ClientMsg::is_mutating)
    );
    room.messages
        .send(ServerMsg::Terminals {
            user: "Kyler".into(),
            terminals: Vec::new(),
        })
        .unwrap();
    pump(cx);
    cx.simulate_click(gpui::point(px(90.), px(62.)), gpui::Modifiers::default());
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-shift-e"
    } else {
        "ctrl-alt-super-e"
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_click(gpui::point(px(300.), px(100.)), gpui::Modifiers::default());
    cx.simulate_keystrokes("z");
    pump(cx);
    assert!(
        runtime.input().ends_with(b"z"),
        "picking yourself must restore local typing"
    );
}

#[gpui::test]
fn person_menu_tags_do_not_repeat_the_name(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let room = Room::start();
    let (_, cx) = open_view(&runtime, &room, cx);
    pump(cx);
    cx.simulate_click(gpui::point(px(90.), px(62.)), gpui::Modifiers::default());
    let named = |name: &str| Person {
        name: name.into(),
        ..person("e2e", true)
    };
    room.messages
        .send(ServerMsg::People {
            people: vec![named("Host"), person("Kyler", false)],
        })
        .unwrap();
    pump(cx);
    assert!(cx.debug_bounds("Host: You").is_some());
    assert!(cx.debug_bounds("Host: You, Host").is_none());
    room.messages
        .send(ServerMsg::People {
            people: vec![named("Alice")],
        })
        .unwrap();
    pump(cx);
    assert!(cx.debug_bounds("Alice: You, Host").is_some());
}

pub(super) fn pump(cx: &mut gpui::VisualTestContext) {
    let end = Instant::now() + Duration::from_millis(250);
    while Instant::now() < end {
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(5));
    }
}

#[gpui::test]
fn sidebar_tooltips_follow_the_selected_machine(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let picture = runtime.directory.path().join("picture.png");
    image::RgbaImage::new(1, 2048).save(&picture).unwrap();
    std::fs::write(
        runtime.directory.path().join("settings.toml"),
        format!(
            "glass = false\nhide_top_bar = false\npicture = '{}'\n",
            picture.display()
        ),
    )
    .unwrap();
    let room = Room::start();
    let (view, cx) = open_view(&runtime, &room, cx);
    pump(cx);
    let local_name = view.read_with(cx, |view, _| {
        let Content::Terminal(link, _) = &view.content else {
            panic!("terminal required")
        };
        super::rename::display_name(link.terminals().first().unwrap())
    });
    assert_ne!(local_name, "Claude");
    for width in [960., 480.] {
        for shown in [true, false] {
            check_machine_tooltips(width, shown, &view, &room, &local_name, cx);
        }
    }
}

fn check_machine_tooltips(
    width: f32,
    shown: bool,
    view: &Entity<SeerWindow>,
    room: &Room,
    local_name: &str,
    cx: &mut gpui::VisualTestContext,
) {
    cx.simulate_resize(gpui::size(px(width), px(640.)));
    pump(cx);
    let people = if cfg!(target_os = "macos") {
        "cmd-shift-e"
    } else {
        "ctrl-alt-super-e"
    };
    cx.simulate_keystrokes(&format!("{people} down enter"));
    pump(cx);
    room.messages
        .send(ServerMsg::Terminals {
            user: "Kyler".into(),
            terminals: vec![TerminalInfo {
                pane: "w1:p1".into(),
                title: Some("Claude".into()),
                ..terminal()
            }],
        })
        .unwrap();
    pump(cx);
    let row = gpui::point(px(70.), px(if width > 500. { 110. } else { 66. }));
    cx.simulate_mouse_move(
        gpui::point(px(400.), px(10.)),
        None,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_move(row, None, gpui::Modifiers::default());
    cx.run_until_parked();
    cx.executor()
        .advance_clock(Duration::from_millis(if shown { 600 } else { 100 }));
    cx.run_until_parked();
    let remote_tip = cx.update(|_, cx| crate::tip::terminal_help("Claude", cx));
    assert_eq!(
        painted_tip(&remote_tip, cx),
        shown,
        "remote hover must work: width={width}, shown={shown}"
    );
    cx.simulate_keystrokes(&format!("{people} enter"));
    pump(cx);
    assert!(view.read_with(
        cx,
        |view, _| matches!(&view.content, Content::Terminal(link, _) if !link.remote())
    ));
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(
        !painted_tip(&remote_tip, cx),
        "remote tooltip must close on return"
    );
    cx.simulate_mouse_move(
        gpui::point(px(400.), px(300.)),
        None,
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_move(
        gpui::point(px(400.), px(10.)),
        None,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_move(row, None, gpui::Modifiers::default());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    let local_tip = cx.update(|_, cx| crate::tip::terminal_help(local_name, cx));
    let painted = cx.update(|_, cx| cx.global::<crate::tip::PaintedTips>().0.clone());
    assert!(
        painted.contains(&local_tip),
        "local tooltip must name the local terminal: {painted:?}"
    );
}

fn painted_tip(text: &SharedString, cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|_, cx| cx.global::<crate::tip::PaintedTips>().0.contains(text))
}

pub(super) fn open_view<'a>(
    runtime: &Runtime,
    room: &Room,
    cx: &'a mut TestAppContext,
) -> (gpui::Entity<SeerWindow>, &'a mut gpui::VisualTestContext) {
    let (mut link, local) = runtime.connect();
    let remote = link.join_room(room.server.clone(), None);
    let mut messages = futures::stream::select(local, remote);
    let (view, cx) = cx.add_window_view(|window, cx: &mut Context<SeerWindow>| {
        crate::assets::load_fonts(cx).unwrap();
        let keys = Keys {
            tabs: cx.focus_handle().tab_stop(true),
            who: cx.focus_handle().tab_stop(true),
            new_terminal: cx.focus_handle().tab_stop(true),
            scroll: ScrollHandle::new(),
            mode: Mode::Pointer,
        };
        window.focus(&keys.tabs);
        SeerWindow {
            content: Content::Terminal(Box::new(link), "You".into()),
            font: crate::terminal_font::default_font(),
            font_size: crate::terminal_font::DEFAULT_SIZE,
            ui_font: "Inter".into(),
            keys,
            input: Input::new(window, cx),
            rows: Rows::default(),
            appearance: crate::appearance::Appearance::load(
                Some(runtime.directory.path().join("settings.toml")),
                cx,
            ),
            room: room::RoomUi::new(Some(room.server.clone()), cx),
            join_screen: None,
            _join: None,
            _tab_time: None,
            editing: None,
            menu: None,
            people_menu: None,
            invite: None,
            server: Some(room.server.clone()),
            _messages: Some(cx.spawn(async move |view, cx| {
                while let Some(event) = messages.next().await {
                    view.update(cx, |view, cx| view.receive(event, cx)).unwrap();
                }
            })),
        }
    });
    (view, cx)
}

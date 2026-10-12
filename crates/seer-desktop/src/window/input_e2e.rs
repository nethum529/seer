use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use gpui::{ClipboardItem, TestAppContext, VisualContext, point, size};
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use tempfile::TempDir;

use super::*;

pub(super) struct Runtime {
    child: Child,
    pub(super) directory: TempDir,
}

impl Runtime {
    pub(super) fn start() -> Self {
        Self::start_with_output("Ready\r\n")
    }

    fn start_with_output(output: &str) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("s449")
            .tempdir_in("/tmp")
            .unwrap();
        let shell = directory.path().join("shell");
        fs::write(&shell, format!(
            "#!/bin/sh\nexec python3 -u -c 'import os,tty\ntty.setraw(0)\nos.write(1,bytes({:?}))\nf=open(\"{}/input\",\"ab\",buffering=0)\nwhile True:\n b=os.read(0,4096)\n f.write(b)\n os.write(1,b)'\n",
            output.as_bytes(),
            directory.path().display()
        )).unwrap();
        fs::set_permissions(&shell, fs::Permissions::from_mode(0o700)).unwrap();
        let binary = std::env::var_os("SEER_RUNTIME_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/seer-runtime")
            });
        let child = Command::new(binary)
            .arg(directory.path().join("socket"))
            .args(["e2e"])
            .arg(shell)
            .arg("e2e")
            .env_remove("SEER_SNAPSHOT_DIR")
            .env_remove("SEER_ROOM_ENDPOINT")
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        Self { child, directory }
    }

    pub(super) fn connect(&self) -> (Link, Messages) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let stream = loop {
            if let Ok(stream) = UnixStream::connect(self.directory.path().join("socket")) {
                break stream;
            }
            assert!(Instant::now() < deadline, "runtime did not start");
            thread::sleep(Duration::from_millis(10));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut socket = seer_net::Socket::from(stream);
        let _: ServerMsg = codec::decode(&mut socket).unwrap();
        codec::encode(&mut socket, &ClientMsg::AttachRuntime).unwrap();
        let tree = loop {
            if let ServerMsg::Tree { tree } = codec::decode(&mut socket).unwrap() {
                break tree;
            }
        };
        seer_net::Stream::set_read_timeout(&socket, None).unwrap();
        Link::open(socket, &tree, "e2e".into(), "E2e".into()).unwrap()
    }

    pub(super) fn input(&self) -> Vec<u8> {
        fs::read(self.directory.path().join("input")).unwrap_or_default()
    }
}

#[gpui::test]
fn room_overlays_block_terminal_pointer_input(cx: &mut TestAppContext) {
    let runtime = Runtime::start_with_output("\x1b[?1003h\x1b[?1006hReady\r\n");
    let (view, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    let layout = cx.replace_root_view(|_, _| PointerLayout { view, width: 960. });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !runtime.directory.path().join("input").exists() {
        cx.run_until_parked();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(30));
    cx.run_until_parked();
    let terminal = point(px(300.), px(100.));
    cx.simulate_click(terminal, gpui::Modifiers::default());
    cx.simulate_keystrokes("q");
    wait_for_input(&runtime, cx, b'q', 1);
    assert!(
        runtime.input().windows(3).any(|bytes| bytes == b"\x1b[<"),
        "the shell must receive SGR mouse reports before an overlay opens"
    );

    for (width, menu_inside, cancel_inside) in [
        (960., false, true),
        (960., true, true),
        (400., false, false),
    ] {
        layout.update(cx, |layout, cx| {
            layout.width = width;
            cx.notify();
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
        cx.run_until_parked();
        let expected = runtime.input();
        let trigger = cx.debug_bounds("room-trigger").unwrap();
        cx.simulate_click(trigger.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        exercise_overlay_pointer(cx, "room-menu", terminal);
        click_overlay_action(cx, "room-leave", menu_inside);
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("room-confirm").is_some(),
            "pointer Leave must open its sheet, over_terminal={menu_inside}"
        );
        exercise_overlay_pointer(cx, "room-confirm", terminal);
        click_overlay_action(cx, "room-cancel", cancel_inside);
        thread::sleep(Duration::from_millis(150));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
        cx.simulate_keystrokes("secondary-shift-b tab z");
        wait_for_input(&runtime, cx, b'z', expected.len() + 1);
        assert_eq!(
            runtime.input(),
            [expected, vec![b'z']].concat(),
            "pointer actions must work without terminal reports, width={width}"
        );
    }
}

struct PointerLayout {
    view: gpui::Entity<SeerWindow>,
    width: f32,
}

impl Render for PointerLayout {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(self.width)).h_full().child(self.view.clone())
    }
}

fn click_overlay_action(cx: &mut gpui::VisualTestContext, selector: &'static str, inside: bool) {
    let bounds = cx.debug_bounds(selector).unwrap();
    let position = point(
        if inside {
            bounds.right() - px(4.)
        } else {
            bounds.left() + px(4.)
        },
        bounds.center().y,
    );
    assert_eq!(
        cx.debug_bounds("terminal").unwrap().contains(&position),
        inside,
        "{selector} must exercise the intended terminal boundary"
    );
    cx.simulate_click(position, gpui::Modifiers::default());
}

fn exercise_overlay_pointer(
    cx: &mut gpui::VisualTestContext,
    overlay: &'static str,
    terminal: gpui::Point<gpui::Pixels>,
) {
    let bounds = cx.debug_bounds(overlay).unwrap();
    let inside = point(bounds.right() - px(20.), bounds.top() + px(22.));
    for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
        cx.simulate_mouse_down(inside, button, gpui::Modifiers::default());
        cx.simulate_mouse_move(
            inside + point(px(-8.), px(8.)),
            button,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_move(terminal, button, gpui::Modifiers::default());
        cx.simulate_mouse_up(terminal, button, gpui::Modifiers::default());
    }
    for position in [inside, terminal] {
        cx.simulate_mouse_move(position, None, gpui::Modifiers::default());
        for direction in [-1., 1.] {
            cx.simulate_event(gpui::ScrollWheelEvent {
                position,
                delta: gpui::ScrollDelta::Lines(point(direction, direction)),
                modifiers: gpui::Modifiers::default(),
                touch_phase: gpui::TouchPhase::Moved,
            });
        }
    }
}

fn wait_for_input(runtime: &Runtime, cx: &mut gpui::VisualTestContext, marker: u8, minimum: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while (runtime.input().len() < minimum || !runtime.input().ends_with(&[marker]))
        && Instant::now() < deadline
    {
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        runtime.input().len() >= minimum && runtime.input().ends_with(&[marker]),
        "the shell must receive the input marker"
    );
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

#[gpui::test]
fn desktop_keys_reach_the_local_shell(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (_, cx) = open_view(&runtime, cx);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !runtime.directory.path().join("input").exists() {
        cx.run_until_parked();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    cx.simulate_click(point(px(300.), px(100.)), gpui::Modifiers::default());
    thread::sleep(Duration::from_millis(30));
    cx.run_until_parked();
    cx.write_to_clipboard(ClipboardItem::new_string("Sentinel".into()));
    cx.simulate_mouse_down(
        point(px(260.), px(53.)),
        MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(px(300.), px(53.)),
        MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(px(300.), px(53.)),
        MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-c"
    } else {
        "ctrl-shift-c"
    });
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("Ready")
    );
    cx.simulate_keystrokes("a enter backspace tab up down left right home end pageup pagedown f1 f5 ctrl-c->\x03 shift-tab ctrl-enter ctrl-backspace");
    let mut expected =
        b"a\r\x7f\t\x1b[A\x1b[B\x1b[D\x1b[C\x1b[H\x1b[F\x1b[5~\x1b[6~\x1bOP\x1b[15~\x03\x1b[Z\r"
            .to_vec();
    expected.push(if cfg!(target_os = "macos") {
        0x7f
    } else {
        0x08
    });
    if cfg!(target_os = "macos") {
        cx.simulate_keystrokes("ctrl-2->2 ctrl-6->6 ctrl-/->/ ctrl-shift-/->/ ctrl-alt-2->2 keypadenter->\x03 ctrl-keypadenter->\x03");
        expected.extend_from_slice(b"26//2\n\x03");
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while runtime.input().len() < expected.len() && Instant::now() < deadline {
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        runtime.input(),
        expected,
        "desktop input must match the default terminal bytes"
    );

    cx.write_to_clipboard(ClipboardItem::new_string("paste\ntext".into()));
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-shift-v"
    });
    thread::sleep(Duration::from_millis(30));
    assert!(runtime.input().ends_with(b"paste\rtext"));

    cx.simulate_keystrokes("secondary-shift-b tab");
    cx.simulate_keystrokes("z");
    thread::sleep(Duration::from_millis(30));
    expected.extend_from_slice(b"paste\rtextz");
    assert_eq!(runtime.input(), expected);
}

fn open_view<'a>(
    runtime: &Runtime,
    cx: &'a mut TestAppContext,
) -> (gpui::Entity<SeerWindow>, &'a mut gpui::VisualTestContext) {
    let (link, mut messages) = runtime.connect();
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
            content: Content::Terminal(Box::new(link), "E2e".into()),
            font: screen::terminal_font(),
            ui_font: "Inter".into(),
            keys,
            input: Input::new(window, cx),
            rows: Rows::default(),
            room: {
                let server: seer::ServerEntry = serde_json::from_value(serde_json::json!({"endpoint":"localhost:1", "alias":"E2e", "user_id":"e2e", "name":"E2e", "credential":"e2e", "current":true})).unwrap();
                let mut room = room::RoomUi::new(Some(server.clone()), cx);
                room.current = Some(seer::rooms::RoomSummary {server, state: seer::rooms::RoomState::Detached, people: Vec::new(), host: false});
                room
            },
            editing: None,
            menu: None,
            people_menu: None,
            invite: None,
            server: None,
            _join: None,
            _tab_time: None,
            _messages: Some(cx.spawn(async move |view, cx| {
                while let Some(event) = messages.next().await {
                    view.update(cx, |view, cx| view.receive(event, cx)).unwrap();
                }
            })),
        }
    });
    (view, cx)
}

#[gpui::test]
fn room_menu_escape_and_leave_cancel_keep_the_shell(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (_, cx) = open_view(&runtime, cx);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !runtime.directory.path().join("input").exists() {
        cx.run_until_parked();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    cx.run_until_parked();
    let trigger = cx
        .debug_bounds("room-trigger")
        .expect("room button must be visible");
    cx.simulate_click(trigger.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-menu").is_some());
    cx.simulate_keystrokes("escape enter");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("room-menu").is_some(),
        "Escape must return focus to the room button"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-confirm").is_some());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("room-confirm").is_none(),
        "Cancel must have initial focus"
    );
    cx.simulate_click(point(px(300.), px(100.)), gpui::Modifiers::default());
    cx.simulate_keystrokes("k");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !runtime.input().ends_with(b"k") && Instant::now() < deadline {
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        runtime.input().ends_with(b"k"),
        "Cancel must keep the shell running"
    );
}

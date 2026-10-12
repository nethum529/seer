use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use gpui::{ClipboardItem, TestAppContext, point};
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use tempfile::TempDir;

use super::*;

struct Runtime {
    child: Child,
    directory: TempDir,
}

impl Runtime {
    fn start() -> Self {
        let directory = tempfile::Builder::new()
            .prefix("s449")
            .tempdir_in("/tmp")
            .unwrap();
        let shell = directory.path().join("shell");
        fs::write(&shell, format!(
            "#!/bin/sh\nexec python3 -u -c 'import os,tty\ntty.setraw(0)\nos.write(1,b\"Ready\\r\\n\")\nf=open(\"{}/input\",\"ab\",buffering=0)\nwhile True:\n b=os.read(0,4096)\n f.write(b)\n os.write(1,b)'\n",
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

    fn connect(&self) -> (Link, Messages) {
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
        Link::open(socket, &tree, "e2e".into(), "E2e".into()).unwrap()
    }

    fn input(&self) -> Vec<u8> {
        fs::read(self.directory.path().join("input")).unwrap_or_default()
    }
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
    let (link, mut messages) = runtime.connect();
    let (_, cx) = cx.add_window_view(|window, cx: &mut Context<SeerWindow>| {
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
            editing: None,
            menu: None,
            _messages: Some(cx.spawn(async move |view, cx| {
                while let Some(event) = messages.next().await {
                    view.update(cx, |view, cx| view.receive(event, cx)).unwrap();
                }
            })),
        }
    });
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

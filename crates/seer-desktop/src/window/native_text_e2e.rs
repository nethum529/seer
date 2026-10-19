use futures::FutureExt;
use gpui::{Keystroke, point, px};
use std::fs;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, Person, PersonState, ServerMsg, codec};

struct App(Child);

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(crate) struct Fixture {
    _runtime: App,
    _directory: tempfile::TempDir,
    evidence: std::path::PathBuf,
    capture: std::path::PathBuf,
}

pub(crate) fn prepare() -> Option<Fixture> {
    if !std::env::args().any(|argument| argument == "--ignored") {
        println!(
            "native_text_pixels ... ignored (requires a macOS display and screen capture access)"
        );
        return None;
    }
    let directory = tempfile::Builder::new()
        .prefix("s504")
        .tempdir_in("/tmp")
        .unwrap();
    let root = directory.path();
    let script = root.join("capture.swift");
    fs::write(&script, include_str!("../../tests/text_pixels.swift")).unwrap();
    let capture = root.join("capture");
    assert!(
        Command::new("swiftc")
            .arg(&script)
            .arg("-o")
            .arg(&capture)
            .status()
            .unwrap()
            .success()
    );
    if !Command::new(&capture)
        .args(["display", ""])
        .status()
        .unwrap()
        .success()
    {
        println!("native_text_pixels ... ignored (requires a macOS display)");
        return None;
    }
    // SAFETY: this test entry runs on the main thread before it starts GPUI or any worker thread.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_STATE_HOME", root.join("state"));
        std::env::remove_var("SEER_USER_ID");
    }
    let evidence = std::env::var_os("TEXT_PIXEL_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("captures"));
    fs::create_dir_all(&evidence).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(mut socket) = connection else { break };
            thread::spawn(move || {
                let Ok(ClientMsg::Hello { .. }) = codec::decode(&mut socket) else {
                    return;
                };
                if codec::encode(
                    &mut socket,
                    &ServerMsg::Welcome {
                        user_id: "host".into(),
                        name: "Host".into(),
                        client_id: "pixels".into(),
                        tree: Tree::new(),
                    },
                )
                .is_err()
                {
                    return;
                }
                while let Ok(message) = codec::decode::<_, ClientMsg>(&mut socket) {
                    let reply = match message {
                        ClientMsg::ListPeople => ServerMsg::People {
                            people: vec![Person {
                                user_id: "host".into(),
                                name: "Host".into(),
                                host: true,
                                online: true,
                                idle_secs: 0,
                                attached_clients: 2,
                                peekable: true,
                                state: PersonState::Active,
                                tabs: 1,
                                foreground: "shell".into(),
                            }],
                        },
                        _ => continue,
                    };
                    if codec::encode(&mut socket, &reply).is_err() {
                        break;
                    }
                }
            });
        }
    });
    let config = root.join("config/seer");
    let state = root.join("state/seer/runtimes/host");
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&state).unwrap();
    fs::write(config.join("servers.toml"), format!(
        "[[servers]]\nendpoint = '{address}'\nalias = 'Pairing'\nuser_id = 'host'\nname = 'Host'\ncredential = 'pixels'\ncurrent = true\n"
    )).unwrap();
    fs::write(
        config.join("settings.toml"),
        "glass = false\nhide_top_bar = false\n",
    )
    .unwrap();
    let runtime_binary = std::env::var_os("SEER_RUNTIME_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("seer-runtime")
        });
    assert!(
        runtime_binary.is_file(),
        "build seer-runtime before the native test"
    );
    let mut runtime = App(Command::new(runtime_binary)
        .arg(state.join("socket"))
        .args(["host", "/bin/sh", "pixels"])
        .env_remove("SEER_ROOM_ENDPOINT")
        .env_remove("SEER_SNAPSHOT_DIR")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !state.join("socket").exists() {
        assert!(Instant::now() < deadline && runtime.0.try_wait().unwrap().is_none());
        thread::sleep(Duration::from_millis(20));
    }
    Some(Fixture {
        _runtime: runtime,
        _directory: directory,
        evidence,
        capture,
    })
}

pub(crate) fn exercise(
    window: gpui::WindowHandle<crate::window::SeerWindow>,
    fixture: Fixture,
    cx: &mut gpui::App,
) {
    cx.spawn(async move |cx| {
        let result = std::panic::AssertUnwindSafe(async {
            if std::env::var_os("TEXT_PIXEL_START_ROOM_ONLY").is_some() {
                super::start_room_native_e2e::exercise(
                    window,
                    &fixture.evidence,
                    &fixture.capture,
                    cx,
                )
                .await;
                return measure(&fixture.capture, &fixture.evidence);
            }
            super::spaces_native_e2e::exercise(
                window,
                fixture._directory.path(),
                &fixture.evidence,
                cx,
            )
            .await;
            super::switch_native_e2e::exercise(window, &fixture.evidence, cx).await;
            for name in [
                "main",
                "people",
                "settings",
                "close-settings",
                "room",
                "sheet",
                "close-sheet",
                "context",
                "tooltip",
                "terminal-tip",
                "sidebar-shown-lines",
                "sidebar-hidden-lines",
                "sidebar-shown-no-lines",
                "sidebar-hidden-no-lines",
            ] {
                window
                    .update(cx, |view, window, cx| {
                        match name {
                            name if name.starts_with("sidebar-") => {
                                view.people_menu = None;
                                view.appearance.settings.hide_top_bar = name.contains("hidden");
                                view.appearance.settings.hide_lines = name.contains("no-lines");
                                view.appearance.top_bar.hovered = false;
                                window.focus(&view.input.focus);
                            }
                            "people" => view.open_people(true, window, cx),
                            "settings" => view.open_settings(true, window, cx),
                            "close-settings" => {
                                view.settings_key(&Keystroke::parse("escape").unwrap(), window, cx);
                            }
                            "room" => {
                                window.focus(&view.room.focus);
                                view.room_key(&Keystroke::parse("enter").unwrap(), window, cx);
                            }
                            "sheet" => {
                                view.room_key(&Keystroke::parse("down").unwrap(), window, cx);
                                view.room_key(&Keystroke::parse("enter").unwrap(), window, cx);
                            }
                            "close-sheet" => {
                                view.room_key(&Keystroke::parse("escape").unwrap(), window, cx);
                            }
                            "context" => {
                                let super::Content::Terminal(link, _) = &view.content else {
                                    panic!("terminal is missing")
                                };
                                let pane = link.shown().unwrap().to_owned();
                                view.name_menu(
                                    &pane,
                                    super::rename::Surface::Tab,
                                    point(px(60.), px(108.)),
                                    window,
                                    cx,
                                );
                            }
                            "terminal-tip" => {
                                view.menu = None;
                                view.keys.mode = crate::sidebar::Mode::Keyboard;
                                window.focus(&view.keys.tabs);
                                cx.notify();
                            }
                            "tooltip" => {
                                view.menu = None;
                                view.keys.mode = crate::sidebar::Mode::Keyboard;
                                window.focus(&view.keys.who);
                                cx.notify();
                            }
                            _ => {}
                        }
                        if !matches!(name, "tooltip" | "terminal-tip") {
                            view.keys.mode = crate::sidebar::Mode::Pointer;
                            cx.notify();
                        }
                    })
                    .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(800))
                    .await;
                window
                    .update(cx, |view, window, cx| {
                        window.refresh();
                        if let Some(frame) = &view.input.frame {
                            frame.request(cx);
                        }
                    })
                    .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let result = Command::new(&fixture.capture)
                    .arg(std::process::id().to_string())
                    .arg(&fixture.evidence)
                    .arg(name)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
            super::sidebar_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                .await;
            super::shortcuts_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                .await;
            super::rebind_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                .await;
            super::recovery_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                .await;
            super::start_room_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                .await;
            measure(&fixture.capture, &fixture.evidence)
        })
        .catch_unwind()
        .await;
        drop(fixture);
        let _ = window.update(cx, |_, window, _| window.remove_window());
        std::process::exit(if matches!(result, Ok(true)) { 0 } else { 1 });
    })
    .detach();
}

pub(crate) fn window_options(
    mut options: gpui::WindowOptions,
    cx: &gpui::App,
) -> gpui::WindowOptions {
    if let Ok(secondary) = std::env::var("TEXT_PIXEL_SECONDARY") {
        let display = cx
            .displays()
            .into_iter()
            .find(|display| {
                display.id() != cx.primary_display().unwrap().id()
                    && (secondary == "1" || u32::from(display.id()).to_string() == secondary)
            })
            .expect("a secondary native display is required");
        options.display_id = Some(display.id());
        options.window_bounds = Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
            Some(display.id()),
            gpui::size(px(960.), px(640.)),
            cx,
        )));
    }
    options
}

fn measure(capture: &std::path::Path, evidence: &std::path::Path) -> bool {
    let result = Command::new(capture)
        .arg("measure")
        .arg(evidence)
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&result.stdout));
    eprintln!("{}", String::from_utf8_lossy(&result.stderr));
    result.status.success()
}

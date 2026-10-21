use futures::FutureExt;
mod options;
use gpui::{Keystroke, point, px};
pub(crate) use options::window_options;
use seer_core::Tree;
use seer_core::proto::{ClientMsg, Person, PersonState, ServerMsg, codec};
use std::fs;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
    let evidence = std::env::var_os("TEXT_PIXEL_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("captures"));
    fs::create_dir_all(&evidence).unwrap();
    if std::env::var_os("PROCESSES_QA").is_some() {
        use std::os::unix::fs::PermissionsExt;
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let ps = bin.join("ps");
        fs::write(&ps, format!(
            "#!/bin/sh\n[ -f '{0}/fail-ps' ] && exit 1\n[ -f '{0}/slow-ps' ] && /bin/sleep 3\n/bin/ps \"$@\" | /usr/bin/awk -v root='{0}' '{{row[NR]=$0; pid[NR]=$1; parent[NR]=$2; if(index($0,root)) own[$1]=1}} END {{for(i=1;i<=NR;i++) if(own[pid[i]] || own[parent[i]]) print row[i]}}'\n",
            root.display()
        )).unwrap();
        fs::set_permissions(ps, fs::Permissions::from_mode(0o700)).unwrap();
        // SAFETY: the native fixture sets its environment before GPUI or worker threads start.
        unsafe {
            std::env::set_var(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            );
        }
    }
    super::font_native_e2e::prepare(root);
    super::color_fixture::prepare(root);
    let mirror = std::env::var_os("VERSION_BANNER_QA")
        .map(|_| super::version_mirror::prepare(evidence.clone()));
    // SAFETY: this test entry runs on the main thread before it starts GPUI or any worker thread.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_STATE_HOME", root.join("state"));
        std::env::remove_var("SEER_USER_ID");
        if let Some((url, _)) = &mirror {
            std::env::set_var("SEER_UPDATE_URL", url);
        }
    }
    if let Some((_, start)) = mirror {
        start();
    }
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
                    if std::env::var_os("SHARE_ALL_QA").is_some() {
                        if super::share_all_fixture::reply(&mut socket, &message).is_err() {
                            break;
                        }
                        continue;
                    }
                    let reply = match message {
                        ClientMsg::ListPeople => ServerMsg::People {
                            people: {
                                let mut people = vec![Person {
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
                                }];
                                if std::env::var_os("REMOVE_QA").is_some() {
                                    let mut guest = people[0].clone();
                                    guest.user_id = "guest".into();
                                    guest.name = "Guest".into();
                                    guest.host = false;
                                    people.push(guest);
                                }
                                people
                            },
                        },
                        ClientMsg::RemovePerson { .. }
                            if std::env::var_os("REMOVE_QA").is_some() =>
                        {
                            ServerMsg::Refused {
                                reason: "Could not remove Guest. Try again.".into(),
                            }
                        }
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
    wait_runtime(&mut runtime, &state.join("socket"));
    Some(Fixture {
        _runtime: runtime,
        _directory: directory,
        evidence,
        capture,
    })
}
fn wait_runtime(runtime: &mut App, socket: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !socket.exists() {
        assert!(Instant::now() < deadline && runtime.0.try_wait().unwrap().is_none());
        thread::sleep(Duration::from_millis(20));
    }
}
pub(crate) fn exercise(
    window: gpui::WindowHandle<crate::window::SeerWindow>,
    fixture: Fixture,
    cx: &mut gpui::App,
) {
    cx.spawn(async move |cx| {
        let result = std::panic::AssertUnwindSafe(async {
            if std::env::var_os("TEXT_PIXEL_START_REVIEW_ONLY").is_some() {
                super::start_review_native_e2e::exercise(
                    window,
                    &fixture.evidence,
                    &fixture.capture,
                    cx,
                )
                .await;
                return true;
            }
            if std::env::var_os("TEXT_PIXEL_START_SCREEN_ONLY").is_some() {
                super::start_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                    .await;
                return measure(&fixture.capture, &fixture.evidence, "measure");
            }
            let server = seer::ServerStore::load()
                .unwrap()
                .default_server()
                .unwrap()
                .clone();
            window
                .update(cx, |view, window, cx| {
                    view.pick_saved_room(server, window, cx)
                })
                .unwrap();
            cx.background_executor()
                .timer(Duration::from_millis(900))
                .await;
            if exercise_room_qa(window, &fixture, cx).await {
                return measure(&fixture.capture, &fixture.evidence, "measure-appearance");
            }
            if std::env::var_os("TEXT_PIXEL_START_ROOM_ONLY").is_some() {
                super::start_room_native_e2e::exercise(
                    window,
                    &fixture.evidence,
                    &fixture.capture,
                    cx,
                )
                .await;
                return measure(&fixture.capture, &fixture.evidence, "measure");
            }
            if std::env::var_os("APPEARANCE_SLIDER_QA").is_some() {
                super::appearance_native_e2e::exercise(
                    window,
                    &fixture.evidence,
                    &fixture.capture,
                    cx,
                )
                .await;
                return measure(&fixture.capture, &fixture.evidence, "measure-appearance");
            }
            super::spaces_native_e2e::exercise(
                window,
                fixture._directory.path(),
                &fixture.evidence,
                cx,
            )
            .await;
            super::switch_native_e2e::exercise(window, &fixture.evidence, cx).await;
            super::commands_native_e2e::exercise(window, &fixture.evidence, cx).await;
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
            super::appearance_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                .await;
            super::shortcuts_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                .await;
            super::rebind_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                .await;
            super::recovery_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                .await;
            super::start_room_native_e2e::exercise(window, &fixture.evidence, &fixture.capture, cx)
                .await;
            measure(&fixture.capture, &fixture.evidence, "measure")
        })
        .catch_unwind()
        .await;
        drop(fixture);
        let _ = window.update(cx, |_, window, _| window.remove_window());
        std::process::exit(if matches!(result, Ok(true)) { 0 } else { 1 });
    })
    .detach();
}
async fn exercise_room_qa(
    window: gpui::WindowHandle<crate::window::SeerWindow>,
    fixture: &Fixture,
    cx: &mut gpui::AsyncApp,
) -> bool {
    let evidence = &fixture.evidence;
    let capture = &fixture.capture;
    let root = fixture._directory.path();
    if std::env::var_os("COLOR_QA").is_some() {
        super::color_native_e2e::exercise(window, evidence, capture, cx).await;
        return true;
    }
    if std::env::var_os("FONT_QA").is_some() {
        super::font_native_e2e::exercise(window, evidence, capture, cx).await;
        return true;
    }
    if std::env::var_os("REMOVE_QA").is_some() {
        super::remove_native_e2e::exercise(window, evidence, capture, cx).await;
        return true;
    }
    if std::env::var_os("SHARE_ALL_QA").is_some() {
        super::share_all_native_e2e::exercise(window, evidence, capture, cx).await;
        return true;
    }
    if std::env::var_os("PROCESSES_QA").is_some() {
        super::processes_native_e2e::exercise(window, root, evidence, capture, cx).await;
        return true;
    }
    if std::env::var_os("SHORTCUT_KEYS_QA").is_some() {
        super::shortcuts_native_e2e::exercise(window, evidence, capture, cx).await;
        super::rebind_native_e2e::exercise(window, evidence, capture, cx).await;
        return true;
    }
    if std::env::var_os("VERSION_BANNER_QA").is_some() {
        super::version_native_e2e::exercise(window, evidence, capture, cx).await;
        return true;
    }
    if std::env::var_os("BANNER_QA").is_some() {
        super::banner_native_e2e::exercise(window, evidence, capture, cx).await;
        return true;
    }
    if std::env::var_os("APPEARANCE_LEGACY_QA").is_some() {
        super::legacy_appearance_native_e2e::exercise(
            window,
            &root.join("config/seer/settings.toml"),
            evidence,
            capture,
            cx,
        )
        .await;
        return true;
    }
    false
}

fn measure(capture: &std::path::Path, evidence: &std::path::Path, mode: &str) -> bool {
    let result = Command::new(capture)
        .arg(mode)
        .arg(evidence)
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&result.stdout));
    eprintln!("{}", String::from_utf8_lossy(&result.stderr));
    result.status.success()
}

use std::{
    fs,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use gpui::{TestAppContext, VisualTestContext, px, size};

use super::{SeerWindow, input_e2e::Runtime};

#[gpui::test]
fn start_room_from_an_empty_window(cx: &mut TestAppContext) {
    if std::env::var_os("SEER_START_E2E_CHILD").is_none() {
        let directory = tempfile::Builder::new()
            .prefix("s531")
            .tempdir_in("/tmp")
            .unwrap();
        let binary =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug");
        let paths = std::env::join_paths(std::iter::once(binary).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )))
        .unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "window::start_room_e2e::start_room_from_an_empty_window",
                "--nocapture",
            ])
            .env("SEER_START_E2E_CHILD", "1")
            .env("XDG_CONFIG_HOME", directory.path().join("config"))
            .env("XDG_STATE_HOME", directory.path().join("state"))
            .env("PATH", paths)
            .env_remove("SEER_USER_ID")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let _cleanup = StartedProcesses;
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::assets::load_fonts(cx).unwrap();
        SeerWindow::new(window, cx)
    });
    cx.simulate_resize(size(px(960.), px(640.)));
    draw(cx);
    let trigger = cx
        .debug_bounds("room-trigger")
        .expect("a window with no room needs a room menu");
    cx.simulate_click(trigger.center(), gpui::Modifiers::default());
    draw(cx);
    if !cfg!(target_os = "linux") {
        assert!(
            cx.debug_bounds("room-start").is_none(),
            "this computer cannot host"
        );
        return;
    }
    let runtime = Runtime::start();
    let (mut original, _) = runtime.connect();
    assert!(
        cx.debug_bounds("room-start").is_some(),
        "Linux must offer Start a room"
    );
    cx.simulate_keystrokes("home enter");
    draw(cx);
    assert!(cx.debug_bounds("room-confirm").is_some());
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert!(
        cx.debug_bounds("room-confirm").is_none(),
        "Cancel has initial focus"
    );
    cx.simulate_keystrokes("enter home enter");
    draw(cx);
    let config = std::path::PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap())
        .join("seer/broker.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "invalid config").unwrap();
    cx.simulate_keystrokes("tab enter");
    wait(cx, |cx| cx.debug_bounds("room-error").is_some());
    assert!(
        cx.debug_bounds("room-confirm").is_some(),
        "a failure keeps the retry sheet"
    );
    let address = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let state = std::path::PathBuf::from(std::env::var_os("XDG_STATE_HOME").unwrap()).join("seer");
    fs::write(&config, format!("listen = '{address}'\npublished_addr = '{address}'\nremote = false\nowner_name = 'Host'\nstate_dir = '{}'\n", state.display())).unwrap();
    cx.simulate_keystrokes("enter");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        draw(cx);
        if view.read_with(cx, |view, _| {
            view.room.current.as_ref().is_some_and(|room| room.host)
        }) {
            break;
        }
        assert!(Instant::now() < deadline, "start did not show a host room");
        thread::sleep(Duration::from_millis(10));
    }
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert!(
        cx.debug_bounds("room-invite").is_some(),
        "the new host can invite at once"
    );
    cx.simulate_keystrokes("home enter");
    draw(cx);
    assert!(cx.debug_bounds("invite-primary").is_some());
    original
        .input(seer_core::InputEvent::Text("k".into()))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !runtime.input().ends_with(b"k") {
        assert!(
            Instant::now() < deadline,
            "the original shell must keep running"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear();
    });
}

fn wait(cx: &mut VisualTestContext, mut ready: impl FnMut(&mut VisualTestContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        draw(cx);
        if ready(cx) {
            return;
        }
        assert!(Instant::now() < deadline, "the start action did not finish");
        thread::sleep(Duration::from_millis(10));
    }
}

struct StartedProcesses;

impl Drop for StartedProcesses {
    fn drop(&mut self) {
        let state =
            std::path::PathBuf::from(std::env::var_os("XDG_STATE_HOME").unwrap()).join("seer");
        let mut paths = vec![state.join("broker.pid")];
        if let Ok(entries) = fs::read_dir(state.join("runtimes")) {
            paths.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path().join("runtime.pid")),
            );
        }
        for path in paths {
            if let Ok(pid) = fs::read_to_string(path) {
                let _ = Command::new("kill").args(["-TERM", pid.trim()]).status();
            }
        }
    }
}

use std::{fs, path::Path, process::Command, time::Duration};

use gpui::{FontWeight, Keystroke, WindowHandle, px, size};

use super::shortcuts_native_e2e::{baseline_probe, text_width};

use super::{Content, SeerWindow, room::RoomUi};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let config = std::path::PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap())
        .join("seer/broker.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(config, "invalid config").unwrap();
    handle
        .update(cx, |view, window, cx| {
            window.resize(size(px(960.), px(640.)));
            view.appearance.motion = None;
            view.appearance.settings.hide_top_bar = false;
            view.appearance.settings.hide_lines = false;
            view.room = RoomUi::new(None, cx);
            view.server = None;
            view._messages = None;
            view.content = Content::Start(super::start::StartScreen::new(cx));
            window.focus(&view.room.focus);
            cx.notify();
        })
        .unwrap();
    let mut samples = Vec::new();
    for (name, keys) in [
        ("start-empty", ""),
        ("start-menu", "enter"),
        ("start-sheet", "enter"),
        ("start-error", "tab enter"),
        ("start-cancel", "escape"),
    ] {
        handle
            .update(cx, |view, window, cx| {
                for key in keys.split_whitespace() {
                    view.room_key(&Keystroke::parse(key).unwrap(), window, cx);
                }
                view.keys.mode = crate::sidebar::Mode::Pointer;
                cx.notify();
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(800))
            .await;
        handle
            .update(cx, |view, window, cx| {
                samples.extend(text_samples(name, window));
                window.refresh();
                if let Some(frame) = &view.input.frame {
                    frame.request(cx);
                }
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
        let output = Command::new(capture)
            .arg(std::process::id().to_string())
            .arg(evidence)
            .arg(name)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::write(
        evidence.join("start-room-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
    let path = evidence.join("start-room-fixture.txt");
    fs::write(
        path,
        "The native fixture uses an invalid broker config to show the retry state.\n",
    )
    .unwrap();
}

fn text_samples(capture: &str, window: &gpui::Window) -> Vec<serde_json::Value> {
    let normal = FontWeight::NORMAL;
    let medium = FontWeight::MEDIUM;
    let trigger = 884. - text_width(window, "No room", 13., normal);
    let confirm = 645. - text_width(window, "Start a room", 13., medium);
    let cancel = confirm - 36. - text_width(window, "Cancel", 13., medium);
    let mut labels = vec![("Room trigger", "No room", trigger, 6., 32., 13., normal)];
    match capture {
        "start-menu" => labels.extend([
            ("Empty room row", "No room", 685., 47., 36., 13., medium),
            (
                "Start menu row",
                "Start a room",
                717.,
                92.,
                36.,
                13.,
                medium,
            ),
        ]),
        "start-sheet" | "start-error" => {
            let offset = if capture == "start-error" { -13. } else { 0. };
            labels.extend([
                (
                    "Start sheet title",
                    "Start a room?",
                    301.,
                    271. + offset,
                    20.,
                    15.,
                    FontWeight::SEMIBOLD,
                ),
                (
                    "Start sheet body",
                    "You will host a room on this computer.",
                    301.,
                    299. + offset,
                    18.,
                    13.,
                    normal,
                ),
                (
                    "Start sheet Cancel",
                    "Cancel",
                    cancel,
                    337. - offset,
                    32.,
                    13.,
                    medium,
                ),
                (
                    "Start sheet confirm",
                    "Start a room",
                    confirm,
                    337. - offset,
                    32.,
                    13.,
                    medium,
                ),
            ]);
            if capture == "start-error" {
                labels.push((
                    "Start error",
                    "Could not start the room. Try again.",
                    301.,
                    312.,
                    18.,
                    12.,
                    normal,
                ));
            }
        }
        _ => {}
    }
    labels
        .into_iter()
        .map(|(name, text, x, y, height, size, weight)| {
            serde_json::json!({
                "capture": capture,
                "name": format!("{capture}: {name}"),
                "box": [x, y, text_width(window, &text[..1], size, weight).floor().max(2.), height],
                "probe": baseline_probe(window, text, x, size, weight),
                "color": if name == "Start error" { Some([248, 113, 113]) } else { None },
            })
        })
        .collect()
}

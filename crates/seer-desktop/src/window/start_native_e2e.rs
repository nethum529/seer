use std::{fs, path::Path, process::Command, time::Duration};

use gpui::{FontWeight, WindowHandle, px, size};

use super::shortcuts_native_e2e::{baseline_probe, text_width};
use super::{Content, SeerWindow};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let mut samples = Vec::new();
    for name in [
        "launch-saved",
        "launch-empty",
        "launch-join",
        "launch-menu",
        "launch-menu-people",
    ] {
        handle
            .update(cx, |view, window, cx| {
                window.resize(size(px(960.), px(640.)));
                view.appearance.settings.glass = false;
                view.appearance.settings.hide_top_bar = false;
                view.appearance.settings.hide_lines = false;
                match name {
                    "launch-saved" => assert!(matches!(view.content, Content::Start(_))),
                    "launch-empty" => {
                        let Content::Start(start) = &mut view.content else {
                            panic!("start screen is missing")
                        };
                        start.saved.clear();
                    }
                    "launch-join" => view.open_join(None, window, cx),
                    "launch-menu" => {
                        view.close_join(window, cx);
                        let server = seer::ServerStore::load()
                            .unwrap()
                            .default_server()
                            .unwrap()
                            .clone();
                        view.joined(server, window, cx);
                    }
                    "launch-menu-people" => {
                        let current = view.room.current.as_mut().unwrap();
                        current.people.push(current.people[0].clone());
                    }
                    _ => unreachable!(),
                }
                cx.notify();
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(900))
            .await;
        if name == "launch-menu" {
            handle
                .update(cx, |view, window, cx| {
                    assert!(matches!(view.content, Content::Terminal(..)));
                    window.focus(&view.room.focus);
                    view.room_key(&gpui::Keystroke::parse("enter").unwrap(), window, cx);
                })
                .unwrap();
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
        }
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
        evidence.join("start-screen-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
}

fn text_samples(capture: &str, window: &gpui::Window) -> Vec<serde_json::Value> {
    let medium = FontWeight::MEDIUM;
    let normal = FontWeight::NORMAL;
    let top = 44. + (640. - 44.) * 0.32;
    let mut labels = Vec::new();
    if capture == "launch-saved" || capture == "launch-empty" {
        labels.extend([
            (
                "Start a room",
                "Start a room",
                480. - text_width(window, "Start a room", 13., medium) / 2.,
                top + 52.,
                32.,
                13.,
                medium,
            ),
            (
                "Join a room",
                "Join a room",
                480. - text_width(window, "Join a room", 13., medium) / 2.,
                top + 92.,
                32.,
                13.,
                medium,
            ),
        ]);
        if capture == "launch-saved" {
            labels.push(("Saved room", "Pairing", 340., top + 148., 36., 13., medium));
        }
    }
    if capture == "launch-join" {
        let right = 660. - 15.;
        let next = right - text_width(window, "Continue", 13., medium);
        let back = next - 38. - text_width(window, "Back", 13., medium);
        labels.extend([
            ("Back", "Back", back, top + 132., 32., 13., medium),
            ("Continue", "Continue", next, top + 132., 32., 13., medium),
        ]);
    }
    if capture.starts_with("launch-menu") {
        let bottom = 598.5;
        let top = bottom - 163. + 5.;
        labels.extend([
            ("Room name", "Pairing", 53., top, 36., 13., medium),
            (
                "People count",
                if capture == "launch-menu" {
                    "1 person"
                } else {
                    "2 people"
                },
                275. - text_width(
                    window,
                    if capture == "launch-menu" {
                        "1 person"
                    } else {
                        "2 people"
                    },
                    12.,
                    normal,
                ),
                top,
                36.,
                12.,
                normal,
            ),
            (
                "Invite action",
                "Invite a person",
                53.,
                top + 45.,
                36.,
                13.,
                medium,
            ),
            ("Stop action", "Stop room", 53., top + 81., 36., 13., medium),
            (
                "Join action",
                "Join a room",
                53.,
                top + 117.,
                36.,
                13.,
                medium,
            ),
        ]);
    }
    let mut samples: Vec<_> = labels
        .into_iter()
        .map(|(name, text, x, y, height, size, weight)| {
            serde_json::json!({
                "capture": capture, "name": format!("{capture}: {name}"),
                "box": [x, y, text_width(window, &text[..1], size, weight).floor().max(2.), height],
                "probe": baseline_probe(window, text, x, size, weight),
                "color": if name == "Stop action" { Some([248, 113, 113]) } else { None },
            })
        })
        .collect();
    if capture == "launch-saved" || capture == "launch-empty" {
        let face = gpui::Font {
            weight: FontWeight::SEMIBOLD,
            ..gpui::font("Inter Display")
        };
        let line = window.text_system().shape_line(
            "Seer".into(),
            px(22.),
            &[gpui::TextRun {
                len: 4,
                font: face,
                color: gpui::rgb(0xffffff).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        samples.push(serde_json::json!({
            "capture": capture, "name": format!("{capture}: Seer"),
            "box": [300., top, f32::from(line.x_for_index(1)).floor(), 28.],
            "probe": [300. + f32::from(line.x_for_index(3)), f32::from(line.width - line.x_for_index(3)).floor()],
        }));
    }
    samples
}

use std::{fs, path::Path, process::Command, time::Duration};

use gpui::{FontWeight, WindowHandle, font, point, px};

use super::SeerWindow;
use crate::{
    appearance::Section,
    shortcuts::{self, Row},
};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |view, window, cx| {
            super::start_review_native_e2e::capture_level(window);
            window.resize(gpui::size(px(960.), px(640.)));
            window.activate_window();
            view.open_settings(true, window, cx);
            view.settings_section(Section::Shortcuts, cx);
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(200))
        .await;
    let native = handle
        .update(cx, |_, window, cx| {
            serde_json::json!({"width": f32::from(window.viewport_size().width), "height": f32::from(window.viewport_size().height), "scale": window.scale_factor(), "display": window.display(cx).map(|display| u32::from(display.id()))})
        })
        .unwrap();
    fs::write(evidence.join("shortcut-window.json"), native.to_string()).unwrap();
    let mut samples = Vec::new();
    let mut inventory = Vec::new();
    for shortcut in shortcuts::SHORTCUTS {
        inventory.push(serde_json::json!({"context": shortcut.context, "name": shortcut.name, "mac": shortcut.keys(shortcuts::Platform::Mac), "linux": shortcut.keys(shortcuts::Platform::Linux)}));
    }
    let rows = shortcuts::rows();
    for (row, item) in rows.iter().enumerate() {
        handle
            .update(cx, |view, _, cx| {
                view.appearance.scroll.scroll_to_top_of_item(row);
                cx.notify();
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(60))
            .await;
        let name = format!("shortcuts-{row}");
        handle.update(cx, |view, window, cx| {
            let scroll = &view.appearance.scroll;
            let bounds = scroll.bounds_for_item(row).unwrap() + scroll.offset();
            let y = f32::from(bounds.top());
            let x = f32::from(bounds.left());
            let height = f32::from(bounds.size.height);
            let mut sample = |text: &str, left: f32, size: f32, weight: FontWeight, label: String| {
                let first = &text[..text.chars().next().unwrap().len_utf8()];
                let width = text_width(window, first, size, weight).floor().max(2.);
                let probe = baseline_probe(window, text, left, size, weight);
                samples.push(serde_json::json!({"capture": name, "name": label, "box": [left, y, width, height], "probe": probe}));
            };
            match item {
                Row::Heading(text) => sample(text, x, 13., FontWeight::SEMIBOLD, format!("Context: {text}")),
                Row::Binding(index) => {
                    let shortcut = &shortcuts::SHORTCUTS[*index];
                    sample(shortcut.name, x, 13., FontWeight::MEDIUM, format!("{} - name", shortcut.name));
                    samples.extend(cap_samples(window, &shortcut.key_caps(cx), f32::from(bounds.right()), y, &name, shortcut.name));
                }
            }
            if row == 0 {
                for (text, x, y, size, weight, label) in [
                    ("Settings", 145., 145., 15., FontWeight::SEMIBOLD, "Settings heading"),
                    ("Appearance", 145., 193., 13., FontWeight::MEDIUM, "Appearance navigation"),
                    ("Shortcuts", 145., 225., 13., FontWeight::MEDIUM, "Shortcuts navigation"),
                    ("Shortcuts", 321., 145., 15., FontWeight::SEMIBOLD, "Shortcuts title"),
                    ("Scroll to see all", 321., 463., 13., FontWeight::NORMAL, "Shortcuts scroll cue"),
                ] {
                    let width = text_width(window, &text[..1], size, weight).floor();
                    let probe = baseline_probe(window, text, x, size, weight);
                    samples.push(serde_json::json!({"capture": name, "name": label, "box": [x,y,width,32.], "probe": probe}));
                }
            }
            window.refresh();
            if let Some(frame) = &view.input.frame { frame.request(cx); }
        }).unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(40))
            .await;
        let output = Command::new(capture)
            .arg(std::process::id().to_string())
            .arg(evidence)
            .arg(&name)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if matches!(item, Row::Binding(0 | 2 | 3)) {
            for state in ["hover", "focus"] {
                let position = handle
                    .update(cx, |view, window, cx| {
                        let Row::Binding(index) = item else {
                            unreachable!()
                        };
                        let bounds = view.appearance.scroll.bounds_for_item(row).unwrap()
                            + view.appearance.scroll.offset();
                        let hovering = state == "hover";
                        view.keys.mode = if hovering {
                            crate::sidebar::Mode::Pointer
                        } else {
                            crate::sidebar::Mode::Keyboard
                        };
                        window.focus(&view.appearance.shortcut_focus[*index]);
                        let position = if hovering {
                            point(bounds.right() - px(10.), bounds.top() + px(16.))
                        } else {
                            point(px(900.), px(620.))
                        };
                        super::appearance_native_e2e::post_mouse(window, position, 5);
                        cx.notify();
                        window.refresh();
                        if let Some(frame) = &view.input.frame {
                            frame.request(cx);
                        }
                        position
                    })
                    .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                handle
                    .update(cx, |_, window, _| {
                        assert_eq!(window.mouse_position(), position)
                    })
                    .unwrap();
                let state_name = format!("{name}-{state}");
                let output = Command::new(capture)
                    .arg(std::process::id().to_string())
                    .arg(evidence)
                    .arg(&state_name)
                    .output()
                    .unwrap();
                assert!(output.status.success());
                let state_samples: Vec<_> = samples
                    .iter()
                    .filter(|sample| sample["capture"] == name)
                    .cloned()
                    .map(|mut sample| {
                        sample["capture"] = state_name.clone().into();
                        sample["name"] =
                            format!("{state}: {}", sample["name"].as_str().unwrap()).into();
                        sample
                    })
                    .collect();
                samples.extend(state_samples);
            }
            handle
                .update(cx, |view, window, cx| {
                    view.keys.mode = crate::sidebar::Mode::Pointer;
                    window.focus(&view.appearance.sections[1]);
                    cx.notify();
                })
                .unwrap();
        }
    }
    fs::write(
        evidence.join("shortcut-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
    fs::write(
        evidence.join("shortcut-inventory.json"),
        serde_json::to_vec_pretty(&inventory).unwrap(),
    )
    .unwrap();
    for width in [321., 479., 600.] {
        handle
            .update(cx, |view, window, cx| {
                view.appearance.scroll.scroll_to_top_of_item(0);
                window.resize(gpui::size(px(width), px(640.)));
                cx.notify();
            })
            .unwrap();
        for (name, row) in [
            (format!("shortcuts-narrow-{width}"), 0),
            (format!("shortcuts-narrow-{width}-bottom"), rows.len() - 4),
        ] {
            handle
                .update(cx, |view, window, cx| {
                    view.appearance.scroll.scroll_to_top_of_item(row);
                    cx.notify();
                    window.refresh();
                    if let Some(frame) = &view.input.frame {
                        frame.request(cx);
                    }
                })
                .unwrap();
            cx.background_executor()
                .timer(Duration::from_millis(250))
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
    }
    handle
        .update(cx, |view, window, cx| {
            window.resize(gpui::size(px(960.), px(640.)));
            view.settings_key(&gpui::Keystroke::parse("escape").unwrap(), window, cx);
            view.appearance.scroll.set_offset(point(px(0.), px(0.)));
        })
        .unwrap();
}

pub(super) fn cap_samples(
    window: &gpui::Window,
    keys: &[Vec<String>],
    right: f32,
    top: f32,
    capture: &str,
    name: &str,
) -> Vec<serde_json::Value> {
    let mut caps = Vec::new();
    for (index, chord) in keys.iter().enumerate() {
        if index > 0 {
            caps.push(("/", text_width(window, "/", 12., FontWeight::NORMAL), false));
        }
        for key in chord {
            caps.push((
                key.as_str(),
                (text_width(window, key, 12., FontWeight::NORMAL) + 14.).max(26.),
                true,
            ));
        }
    }
    let width = caps.iter().map(|(_, width, _)| width).sum::<f32>()
        + 4. * caps.len().saturating_sub(1) as f32;
    let mut left = right - width;
    let mut samples = Vec::new();
    for (index, (key, width, cap)) in caps.into_iter().enumerate() {
        if cap && key.chars().any(char::is_alphanumeric) {
            let x = left + (width - text_width(window, key, 12., FontWeight::NORMAL)) / 2.;
            let first = &key[..key.chars().next().unwrap().len_utf8()];
            samples.push(serde_json::json!({
                "capture": capture,
                "name": format!("{name} - key {index}: {key}"),
                "box": [x, top + 6., text_width(window, first, 12., FontWeight::NORMAL).floor().max(2.), 20.],
                "probe": baseline_probe(window, key, x, 12., FontWeight::NORMAL),
                "cap": [left, top + 5., width, 22.],
            }));
        }
        left += width + 4.;
    }
    samples
}

pub(super) fn text_width(window: &gpui::Window, text: &str, size: f32, weight: FontWeight) -> f32 {
    let mut face = font("Inter");
    face.weight = weight;
    f32::from(
        window
            .text_system()
            .shape_line(
                text.to_owned().into(),
                px(size),
                &[gpui::TextRun {
                    len: text.len(),
                    font: face,
                    color: gpui::rgb(0xffffff).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width,
    )
}

pub(super) fn baseline_probe(
    window: &gpui::Window,
    text: &str,
    x: f32,
    size: f32,
    weight: FontWeight,
) -> [f32; 3] {
    let flat = text
        .char_indices()
        .find(|(_, letter)| "EFHIKLMNPRTfhiklmnr".contains(*letter));
    let (index, letter) = flat.unwrap_or_else(|| text.char_indices().next().unwrap());
    let overshoot = if flat.is_some() {
        0.
    } else {
        let mut face = font("Inter");
        face.weight = weight;
        let text = window.text_system();
        let bounds = text
            .typographic_bounds(text.resolve_font(&face), px(size), letter)
            .unwrap();
        -f32::from(bounds.origin.y).min(0.)
    };
    [
        x + text_width(window, &text[..index], size, weight),
        text_width(window, &letter.to_string(), size, weight)
            .floor()
            .max(2.),
        overshoot,
    ]
}

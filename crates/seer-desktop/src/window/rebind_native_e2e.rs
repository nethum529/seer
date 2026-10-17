use super::{
    Content, SeerWindow,
    shortcuts_native_e2e::{baseline_probe, text_width},
};
use crate::appearance::Section;
use gpui::{FontWeight, Keystroke, WindowHandle};
use std::{fs, path::Path, process::Command, time::Duration};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |view, window, cx| {
            view.open_settings(true, window, cx);
            view.settings_section(Section::Shortcuts, cx);
            view.start_capture(0, window, cx);
        })
        .unwrap();
    let mut samples = Vec::new();
    for state in [
        "capture",
        "conflict",
        "reserved",
        "unsupported",
        "save-error",
        "changed",
        "restart",
    ] {
        handle
            .update(cx, |view, window, cx| match state {
                "conflict" => super::sidebar_native_e2e::post_shortcut(0x0b, 1 << 20),
                "reserved" => {
                    super::sidebar_native_e2e::post_shortcut(0x31, 1 << 20);
                }
                "unsupported" => {
                    view.settings_key(&Keystroke::parse("ctrl-f21").unwrap(), window, cx);
                }
                "save-error" => {
                    let path = view.appearance.path.take();
                    view.settings_key(&Keystroke::parse("cmd-y").unwrap(), window, cx);
                    view.appearance.path = path;
                }
                "changed" => super::sidebar_native_e2e::post_shortcut(0x10, 1 << 20),
                "restart" => {
                    let path = view.appearance.path.clone();
                    view.appearance = crate::appearance::Appearance::load(path, cx);
                    view.open_settings(true, window, cx);
                    view.settings_section(Section::Shortcuts, cx);
                    view.appearance.scroll.scroll_to_top_of_item(0);
                }
                _ => {}
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(200))
            .await;
        if state == "restart" {
            handle
                .update(cx, |view, _, cx| {
                    view.appearance.scroll.set_offset(gpui::Point::default());
                    cx.notify();
                })
                .unwrap();
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;
        }
        let name = format!("rebind-{state}");
        handle.update(cx, |view, window, cx| {
            let row = 1;
            let bounds = view.appearance.scroll.bounds_for_item(row).unwrap() + view.appearance.scroll.offset();
            let x = f32::from(bounds.left());
            let y = f32::from(bounds.top());
            let right = f32::from(bounds.right());
            let capturing = view.appearance.capture == Some(0);
            assert_eq!(capturing, matches!(state, "capture" | "conflict" | "reserved" | "unsupported" | "save-error"));
            if state == "conflict" { assert!(view.appearance.shortcut_error.as_ref().unwrap().1.contains("Show or hide sidebar")); }
            if state == "reserved" { assert!(view.appearance.shortcut_error.as_ref().unwrap().1.contains("Reserved for terminal input")); }
            let keys = if capturing { "Press new keys".to_owned() } else { crate::shortcuts::SHORTCUTS[0].effective_keys(crate::shortcuts::Platform::current(), cx) };
            if !capturing { assert_eq!(keys, "Cmd Y"); }
            let action = if capturing { "Cancel" } else { "Reset" };
            let action_width = text_width(window, action, 13., FontWeight::MEDIUM) + 16.;
            let key_width = text_width(window, &keys, 13., FontWeight::NORMAL);
            let mut sample = |text: &str, left: f32, top: f32, height: f32, weight: FontWeight, label: &str| {
                let first = &text[..text.chars().next().unwrap().len_utf8()];
                samples.push(serde_json::json!({"capture": name, "name": format!("{state}: {label}"), "box": [left,top,text_width(window,first,13.,weight).floor().max(2.),height], "probe": baseline_probe(window,text,left,13.,weight), "color": if label == "message" && state != "capture" { Some([248,113,113]) } else { None }}));
            };
            sample("New terminal",x,y,32.,FontWeight::MEDIUM,"name");
            sample(&keys,right-action_width-8.-8.-key_width,y,32.,FontWeight::NORMAL,"keys");
            sample(action,right-action_width+8.,y,32.,FontWeight::MEDIUM,action);
            if capturing {
                let text = view.appearance.shortcut_error.as_ref().map_or("Escape cancels", |(_, text)| text.as_str());
                sample(text,x,y+36.,32.,FontWeight::NORMAL,"message");
            }
            cx.notify();
            window.refresh();
            if let Some(frame) = &view.input.frame { frame.request(cx); }
        }).unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(60))
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
    }
    for width in [321., 479., 600.] {
        handle
            .update(cx, |view, window, cx| {
                window.resize(gpui::size(gpui::px(width), gpui::px(640.)));
                view.start_capture(0, window, cx);
                view.appearance.scroll.set_offset(gpui::Point::default());
            })
            .unwrap();
        for state in ["capture", "conflict"] {
            if state == "conflict" {
                handle
                    .update(cx, |_, _, _| {
                        super::sidebar_native_e2e::post_shortcut(0x0b, 1 << 20)
                    })
                    .unwrap();
            }
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let output = Command::new(capture)
                .arg(std::process::id().to_string())
                .arg(evidence)
                .arg(format!("rebind-{state}-{width}"))
                .output()
                .unwrap();
            assert!(output.status.success());
        }
    }
    handle
        .update(cx, |view, window, cx| {
            window.resize(gpui::size(gpui::px(960.), gpui::px(640.)));
            view.cancel_capture(window, cx);
        })
        .unwrap();
    let path = evidence.join("shortcut-samples.json");
    let mut all: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    all.extend(samples);
    fs::write(path, serde_json::to_vec_pretty(&all).unwrap()).unwrap();
    handle
        .update(cx, |view, window, cx| {
            view.settings_key(&Keystroke::parse("escape").unwrap(), window, cx);
            super::sidebar_native_e2e::post_shortcut(0x10, 1 << 20);
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    handle
        .update(cx, |view, window, cx| {
            let Content::Terminal(link, _) = &view.content else {
                panic!("terminal required")
            };
            assert_eq!(
                link.terminals().len(),
                2,
                "native Cmd Y must create a terminal after reload"
            );
            view.change_shortcut(0, None, window, cx);
        })
        .unwrap();
    fs::write(evidence.join("native-rebind.txt"), "Cmd B rejected as a conflict. Native Cmd Space rejected as reserved. Native Cmd Y captured, saved, reloaded, and used to create a terminal. Reset restored the default.\n").unwrap();
}

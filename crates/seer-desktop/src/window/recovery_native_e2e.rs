use super::{
    SeerWindow,
    shortcuts_native_e2e::{baseline_probe, text_width},
};
use crate::{
    appearance::{Appearance, Section},
    shortcuts,
};
use gpui::{FontWeight, WindowHandle, px, size};
use std::{fs, path::Path, process::Command, time::Duration};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |view, window, cx| {
            let path = view.appearance.path.clone().unwrap();
            let mut settings = view.appearance.settings.clone();
            settings
                .shortcut_overrides
                .insert("mac.new_terminal".into(), "ctrl-c".into());
            settings
                .shortcut_overrides
                .insert("mac.toggle_sidebar".into(), "tab".into());
            settings.save_to(&path).unwrap();
            view.appearance = Appearance::load(Some(path), cx);
            view.open_settings(true, window, cx);
            view.settings_section(Section::Shortcuts, cx);
            window.refresh();
            if let Some(frame) = &view.input.frame {
                frame.request(cx);
            }
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    let mut samples = Vec::new();
    handle.update(cx, |view, window, cx| {
        for (index, text) in &view.appearance.shortcut_load_errors {
            let row = index + 1;
            let bounds = view.appearance.scroll.bounds_for_item(row).unwrap() + view.appearance.scroll.offset();
            let x = f32::from(bounds.left());
            let y = f32::from(bounds.top());
            let shortcut = &shortcuts::SHORTCUTS[*index];
            let keys = shortcut.effective_keys(shortcuts::Platform::current(), cx);
            for (label, text, left, top, weight, color) in [
                ("name", shortcut.name, x, y, FontWeight::MEDIUM, None),
                ("keys", keys.as_str(), f32::from(bounds.right())-8.-text_width(window, &keys, 13., FontWeight::NORMAL), y, FontWeight::NORMAL, None),
                ("message", text.as_str(), x, y+36., FontWeight::NORMAL, Some([248,113,113])),
            ] {
                samples.push(serde_json::json!({"capture":"recovery", "name":format!("recovery {}: {label}",shortcut.name), "box":[left,top,text_width(window,&text[..1],13.,weight).floor().max(2.),32.], "probe":baseline_probe(window,text,left,13.,weight), "color":color}));
            }
        }
        window.refresh();
    }).unwrap();
    for width in [960., 321., 479., 600.] {
        handle
            .update(cx, |view, window, cx| {
                window.resize(size(px(width), px(640.)));
                view.appearance.scroll.set_offset(gpui::Point::default());
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
        let name = if width == 960. {
            "recovery".into()
        } else {
            format!("recovery-{width}")
        };
        let output = Command::new(capture)
            .arg(std::process::id().to_string())
            .arg(evidence)
            .arg(name)
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    let path = evidence.join("shortcut-samples.json");
    let mut all: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    all.extend(samples);
    fs::write(path, serde_json::to_vec_pretty(&all).unwrap()).unwrap();
}

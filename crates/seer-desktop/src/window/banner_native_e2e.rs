use std::{fs, path::Path, process::Command, time::Duration};

use gpui::{FontWeight, WindowHandle, px, size};

use super::{
    SeerWindow,
    shortcuts_native_e2e::{baseline_probe, text_width},
};
use crate::{link::Event, room::transport::RoomEvent};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let mut samples = Vec::new();
    for (name, width, lost) in [
        ("connected", 960., false),
        ("banner", 960., true),
        ("reconnected", 960., false),
        ("banner-narrow", 680., true),
        ("banner-small", 320., true),
        ("stopped", 960., false),
    ] {
        handle
            .update(cx, |view, window, cx| {
                window.resize(size(px(width), px(640.)));
                if lost {
                    view.receive(Ok(Event::Room(RoomEvent::Lost)), cx);
                } else {
                    view.receive(
                        Ok(Event::Room(RoomEvent::Connected(
                            std::sync::mpsc::channel().0,
                        ))),
                        cx,
                    );
                }
                if name == "stopped" {
                    view.receive(Ok(Event::Room(RoomEvent::Stopped)), cx);
                }
                assert!(
                    view.input.focus.is_focused(window),
                    "banner must keep terminal focus"
                );
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(350))
            .await;
        handle.update(cx, |_, window, _| {
            let scale = window.scale_factor();
            println!("{name}: native scale {scale}, width {width}");
            let normal = FontWeight::NORMAL;
            if name == "banner" {
                for (label, text, x, y, height, weight) in [
                    ("Banner", "Cannot reach the room. Shells on this computer still run.", 260., 44., 34., normal),
                    ("Title", "Shell", 600., 7.5, 28., FontWeight::MEDIUM),
                    ("Sidebar name", "Host", 44., 52., 36., FontWeight::SEMIBOLD),
                    ("Sidebar tab", "Shell", 44., 92., 33., FontWeight::MEDIUM),
                    ("New terminal", "New terminal", 44., 131., 32., FontWeight::MEDIUM),
                ] {
                    let x = if label == "Title" { 604. - text_width(window, text, 13., weight) / 2. } else { x };
                    samples.push(serde_json::json!({"capture": name, "name":label, "logical_width":width, "box":[x,y,text_width(window,&text[..1],13.,weight).floor().max(2.),height], "probe":baseline_probe(window,text,x,13.,weight)}));
                }
            }
        }).unwrap();
        let result = Command::new(capture)
            .arg(std::process::id().to_string())
            .arg(evidence)
            .arg(name)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::write(
        evidence.join("banner-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
}

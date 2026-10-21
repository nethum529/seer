use std::{fs, path::Path, time::Duration};

use gpui::{FontWeight, WindowHandle, point, px, size};

use super::{
    SeerWindow,
    appearance_native_e2e::{post_mouse, snapshot},
    banner::UpdateState,
    shortcuts_native_e2e::{baseline_probe, text_width},
    sidebar_native_e2e::post_shortcut,
};
use crate::{link::Event, room::transport::RoomEvent};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |view, window, cx| {
            window.activate_window();
            window.focus(&view.input.focus);
            view.receive(
                Ok(Event::Room(RoomEvent::VersionMismatch(
                    seer::VersionRefusal {
                        server: "0.7.0".into(),
                        client: "0.6.0".into(),
                    },
                ))),
                cx,
            );
            view.receive(Ok(Event::Room(RoomEvent::Lost)), cx);
            assert!(view.input.focus.is_focused(window));
        })
        .unwrap();
    cx.update(|cx| cx.activate(true)).unwrap();
    let mut samples = Vec::new();
    record(
        handle,
        "mismatch",
        960.,
        evidence,
        capture,
        &mut samples,
        cx,
    )
    .await;
    handle
        .update(cx, |_, window, _| {
            post_mouse(window, point(px(910.), px(68.)), 1);
            post_mouse(window, point(px(910.), px(68.)), 2);
        })
        .unwrap();
    wait(handle, cx, |view| {
        matches!(view.room.version.state, UpdateState::Updating)
    })
    .await;
    record(
        handle,
        "updating",
        960.,
        evidence,
        capture,
        &mut samples,
        cx,
    )
    .await;
    assert!(
        evidence.join("request-1").exists(),
        "pointer Update must run the updater"
    );
    handle
        .update(cx, |_, window, _| {
            post_mouse(window, point(px(910.), px(68.)), 1);
            post_mouse(window, point(px(910.), px(68.)), 2);
        })
        .unwrap();
    fs::write(evidence.join("release-1"), []).unwrap();
    wait(handle, cx, |view| {
        matches!(view.room.version.state, UpdateState::Failed(_))
    })
    .await;
    record(handle, "failure", 960., evidence, capture, &mut samples, cx).await;
    assert!(
        !evidence.join("request-2").exists(),
        "Updating must be disabled"
    );
    for width in [680., 320.] {
        record(
            handle,
            &format!("failure-{width}"),
            width,
            evidence,
            capture,
            &mut samples,
            cx,
        )
        .await;
    }
    handle
        .update(cx, |_, window, _| {
            window.resize(size(px(960.), px(640.)));
            post_shortcut(0x0b, (1 << 20) | (1 << 17));
            post_shortcut(0x30, 0);
        })
        .unwrap();
    pause(cx).await;
    handle
        .update(cx, |view, window, _| {
            assert!(
                view.room.version.focus.is_focused(window),
                "Tab from the sidebar must reach Update"
            );
            post_shortcut(0x24, 0);
        })
        .unwrap();
    wait(handle, cx, |view| {
        matches!(view.room.version.state, UpdateState::Updating)
    })
    .await;
    pause(cx).await;
    assert!(
        evidence.join("request-2").exists(),
        "keyboard Update must run the updater"
    );
    fs::write(evidence.join("release-2"), []).unwrap();
    wait(handle, cx, |view| {
        matches!(view.room.version.state, UpdateState::Updated)
    })
    .await;
    assert!(
        evidence.join("installed").exists(),
        "the shared updater must run the fixture installer"
    );
    record(handle, "success", 960., evidence, capture, &mut samples, cx).await;
    fs::write(
        evidence.join("banner-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
}

async fn wait(
    handle: WindowHandle<SeerWindow>,
    cx: &mut gpui::AsyncApp,
    ready: impl Fn(&SeerWindow) -> bool,
) {
    for _ in 0..40 {
        if handle.update(cx, |view, _, _| ready(view)).unwrap() {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
    }
    panic!("the version banner did not reach its expected state");
}

async fn pause(cx: &gpui::AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(350))
        .await;
}

async fn record(
    handle: WindowHandle<SeerWindow>,
    name: &str,
    width: f32,
    evidence: &Path,
    capture: &Path,
    samples: &mut Vec<serde_json::Value>,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |view, window, cx| {
            window.resize(size(px(width), px(640.)));
            assert!(
                view.input.focus.is_focused(window),
                "status must keep terminal focus"
            );
            cx.notify();
        })
        .unwrap();
    pause(cx).await;
    if width == 960. {
        handle.update(cx, |view, window, _| {
            let (text, action) = match &view.room.version.state {
                UpdateState::Ready => ("This room uses Seer 0.7.0. You have 0.6.0.", Some("Update")),
                UpdateState::Updating => ("This room uses Seer 0.7.0. You have 0.6.0.", Some("Updating...")),
                UpdateState::Failed(error) => (error.as_str(), Some("Update")),
                UpdateState::Updated => ("Seer is updated. Reopen Seer to use it.", None),
            };
            let mut sample = |label: &str, text: &str, x: f32, y: f32, height: f32, weight| {
                samples.push(serde_json::json!({"capture":name,"name":format!("{name} {label}"),"logical_width":width,"box":[x,y,text_width(window,&text[..1],13.,weight).floor(),height],"probe":baseline_probe(window,text,x,13.,weight)}));
            };
            sample("Banner", text, 260., 44., 48., FontWeight::NORMAL);
            if let Some(action) = action {
                let x = 948. - 14. - text_width(window, action, 13., FontWeight::MEDIUM);
                sample("Action", action, x, 52., 32., FontWeight::MEDIUM);
            }
            if name == "mismatch" {
                for (label, text, x, y, height, weight) in [
                    ("Title", "Shell", 480. - text_width(window, "Shell", 13., FontWeight::MEDIUM) / 2., 7.5, 28., FontWeight::MEDIUM),
                    ("Sidebar name", "Host", 44., 52., 36., FontWeight::SEMIBOLD),
                    ("Sidebar tab", "Shell", 44., 92., 33., FontWeight::MEDIUM),
                    ("New terminal", "New terminal", 44., 131., 32., FontWeight::MEDIUM),
                ] { sample(label, text, x, y, height, weight); }
            }
            println!("{name}: native scale {}", window.scale_factor());
        }).unwrap();
    }
    snapshot(capture, evidence, name);
}

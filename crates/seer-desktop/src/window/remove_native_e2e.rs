use super::{
    SeerWindow,
    shortcuts_native_e2e::{baseline_probe, text_width},
};
use gpui::{FontWeight, Keystroke, WindowHandle};
use std::{fs, path::Path, process::Command, time::Duration};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    cx.background_executor()
        .timer(Duration::from_millis(700))
        .await;
    let mut samples = Vec::new();
    for name in ["remove-menu", "remove-sheet", "remove-error", "removed"] {
        handle
            .update(cx, |view, window, cx| {
                match name {
                    "remove-menu" => view.open_people(true, window, cx),
                    "remove-sheet" => {
                        for key in ["end", "enter"] {
                            view.people_key(&Keystroke::parse(key).unwrap(), window, cx);
                        }
                    }
                    "remove-error" => {
                        for key in ["tab", "enter"] {
                            view.room_key(&Keystroke::parse(key).unwrap(), window, cx);
                        }
                    }
                    "removed" => {
                        view.room.current.as_mut().unwrap().host = false;
                        view.room.server.as_mut().unwrap().name = "Guest".into();
                        view.receive(
                            Ok(crate::link::Event::Room(
                                crate::room::transport::RoomEvent::Removed,
                            )),
                            cx,
                        );
                    }
                    _ => unreachable!(),
                }
                cx.notify();
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(700))
            .await;
        handle
            .update(cx, |view, window, cx| {
                if name == "removed" {
                    assert!(matches!(&view.content, super::Content::Message(text) if text.as_ref() == super::room::REMOVED));
                }
                if name == "remove-error" {
                    assert!(
                        !view.room.pending && view.room.modal(),
                        "the peer must refuse the removal"
                    );
                }
                println!("{name}: native scale {}", window.scale_factor());
                samples.extend(text_samples(name, window));
                window.refresh();
                if let Some(frame) = &view.input.frame { frame.request(cx); }
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(350))
            .await;
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
        evidence.join("remove-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
}

fn text_samples(capture: &str, window: &gpui::Window) -> Vec<serde_json::Value> {
    let normal = FontWeight::NORMAL;
    let medium = FontWeight::MEDIUM;
    let mut labels = Vec::new();
    match capture {
        "remove-menu" => labels.extend([
            ("Host", "Host", 53., 97., 36., 13., medium, false),
            ("Guest", "Guest", 53., 133., 36., 13., medium, false),
            ("Share", "Share control", 53., 169., 36., 12., normal, false),
            ("Remove", "Remove", 53., 205., 36., 13., medium, true),
        ]),
        "remove-sheet" | "remove-error" => {
            let offset = if capture == "remove-error" { -13. } else { 0. };
            let confirm = 645. - text_width(window, "Remove", 13., medium);
            let cancel = confirm - 36. - text_width(window, "Cancel", 13., medium);
            labels.extend([
                (
                    "Title",
                    "Remove Guest?",
                    301.,
                    262. + offset,
                    20.,
                    15.,
                    FontWeight::SEMIBOLD,
                    false,
                ),
                (
                    "Body",
                    "Guest leaves the room. Their shells keep running on their computer.",
                    301.,
                    290. + offset,
                    36.,
                    13.,
                    normal,
                    false,
                ),
                (
                    "Cancel",
                    "Cancel",
                    cancel,
                    346. - offset,
                    32.,
                    13.,
                    medium,
                    false,
                ),
                (
                    "Confirm",
                    "Remove",
                    confirm,
                    346. - offset,
                    32.,
                    13.,
                    medium,
                    false,
                ),
            ]);
            if capture == "remove-error" {
                labels.push((
                    "Error",
                    "Could not remove Guest. Try again.",
                    301.,
                    321.,
                    18.,
                    12.,
                    normal,
                    true,
                ));
            }
        }
        "removed" => {
            for (name, text, y, weight) in [
                ("Title", "The host removed you from this room", 322., medium),
                ("Body", "Ask the host for a new invite.", 344., normal),
            ] {
                let x = 604. - text_width(window, text, 13., weight) / 2.;
                labels.push((name, text, x, y, 18., 13., weight, false));
            }
        }
        _ => {}
    }
    labels.into_iter().map(|(name,text,x,y,height,size,weight,danger)| serde_json::json!({"capture":capture,"name":format!("{capture} {name}"),"box":[x,y,text_width(window,&text[..1],size,weight).floor().max(2.),height],"probe":baseline_probe(window, if name == "Body" && capture != "removed" { "computer." } else { text }, x,size,weight),"color":if danger {Some([248,113,113])} else {None}})).collect()
}

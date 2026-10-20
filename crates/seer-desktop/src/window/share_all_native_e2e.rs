use std::{fs, path::Path, time::Duration};

use gpui::{FontWeight, Keystroke, WindowHandle, point, px};

use super::appearance_native_e2e::{post_mouse, snapshot};
use super::shortcuts_native_e2e::{baseline_probe, text_width};
use super::{Content, SeerWindow};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let mut samples = Vec::new();
    handle
        .update(cx, |view, window, cx| {
            window.focus(&view.room.focus);
            view.room_key(&Keystroke::parse("enter").unwrap(), window, cx);
        })
        .unwrap();
    pause(cx).await;
    snapshot(capture, evidence, "share-menu");
    handle
        .update(cx, |_, window, _| {
            println!("share QA native scale {}", window.scale_factor());
            for (name, text, y) in [
                ("Room name", "Pairing", 368.5),
                ("Invite", "Invite a person", 413.5),
                ("Share all", "Share control with everyone", 449.5),
                ("Stop sharing", "Stop sharing control", 485.5),
                ("Stop room", "Stop room", 521.5),
                ("Join", "Join a room", 557.5),
            ] {
                sample(
                    window,
                    "share-menu",
                    name,
                    text,
                    [53., y, 36., 13.],
                    FontWeight::MEDIUM,
                    &mut samples,
                );
            }
        })
        .unwrap();
    handle
        .update(cx, |view, window, cx| {
            view.room_key(&Keystroke::parse("down").unwrap(), window, cx);
            view.room_key(&Keystroke::parse("enter").unwrap(), window, cx);
        })
        .unwrap();
    pause(cx).await;
    snapshot(capture, evidence, "share-sheet");
    handle
        .update(cx, |_, window, _| {
            let medium = FontWeight::MEDIUM;
            let confirm = 645. - text_width(window, "Share control", 13., medium);
            let cancel = confirm - 36. - text_width(window, "Cancel", 13., medium);
            for (name, text, bounds, weight) in [
                (
                    "Title",
                    "Share control with everyone?",
                    [301., 271., 20., 15.],
                    FontWeight::SEMIBOLD,
                ),
                (
                    "Body",
                    "Every person in this room can type into your terminals.",
                    [301., 299., 18., 13.],
                    FontWeight::NORMAL,
                ),
                ("Cancel", "Cancel", [cancel, 337., 32., 13.], medium),
                (
                    "Confirm",
                    "Share control",
                    [confirm, 337., 32., 13.],
                    medium,
                ),
            ] {
                sample(
                    window,
                    "share-sheet",
                    name,
                    text,
                    bounds,
                    weight,
                    &mut samples,
                );
            }
        })
        .unwrap();
    handle
        .update(cx, |view, window, cx| {
            view.room_key(&Keystroke::parse("tab").unwrap(), window, cx);
            view.room_key(&Keystroke::parse("enter").unwrap(), window, cx);
        })
        .unwrap();
    pause(cx).await;
    for (name, expected) in [("people-shared", true), ("people-stopped", false)] {
        handle
            .update(cx, |view, window, cx| {
                let Content::Terminal(link, _) = &view.content else {
                    panic!("terminal missing")
                };
                for user in ["Kyler", "Maya"] {
                    assert_eq!(link.people.shares_control(user), expected);
                }
                view.open_people(true, window, cx);
            })
            .unwrap();
        pause(cx).await;
        snapshot(capture, evidence, name);
        handle
            .update(cx, |view, window, cx| {
                if expected {
                    for (name, text, y, height, size, weight) in [
                        ("Person", "Kyler", 133., 36., 13., FontWeight::MEDIUM),
                        (
                            "Switch",
                            "Share control",
                            169.,
                            36.,
                            12.,
                            FontWeight::NORMAL,
                        ),
                        ("Second person", "Maya", 241., 36., 13., FontWeight::MEDIUM),
                        (
                            "Second switch",
                            "Share control",
                            277.,
                            36.,
                            12.,
                            FontWeight::NORMAL,
                        ),
                    ] {
                        sample(
                            window,
                            "people-shared",
                            name,
                            text,
                            [53., y, height, size],
                            weight,
                            &mut samples,
                        );
                    }
                }
                view.people_key(&Keystroke::parse("escape").unwrap(), window, cx);
                if expected {
                    window.focus(&view.room.focus);
                    view.room_key(&Keystroke::parse("enter").unwrap(), window, cx);
                }
            })
            .unwrap();
        if expected {
            pause(cx).await;
            handle
                .update(cx, |_, window, _| {
                    post_mouse(window, point(px(150.), px(503.5)), 1);
                    post_mouse(window, point(px(150.), px(503.5)), 2);
                })
                .unwrap();
            pause(cx).await;
        }
    }
    fs::write(
        evidence.join("share-all-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
}

async fn pause(cx: &mut gpui::AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(500))
        .await;
}

fn sample(
    window: &gpui::Window,
    capture: &str,
    name: &str,
    text: &str,
    bounds: [f32; 4],
    weight: FontWeight,
    samples: &mut Vec<serde_json::Value>,
) {
    let [x, y, height, size] = bounds;
    samples.push(serde_json::json!({
        "capture":capture, "name":format!("{capture}: {name}"),
        "box":[x,y,text_width(window,&text[..1],size,weight).floor().max(2.),height],
        "probe":baseline_probe(window,text,x,size,weight),
        "color": if name == "Stop room" { Some([248,113,113]) } else { None },
    }));
}

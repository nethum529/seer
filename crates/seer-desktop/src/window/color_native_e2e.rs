use super::{Content, SeerWindow, color_fixture as fixture};
use gpui::WindowHandle;
use std::{fs, path::Path, time::Duration};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |view, window, cx| {
            view._messages = None;
            view.appearance.replace_picture(None, window);
            let Content::Terminal(link, _) = &view.content else {
                panic!("missing terminal")
            };
            let mut raw = seer_runtime::PaneGrid::new(80, 24);
            let mut text = "\x1b[2J\x1b[H".to_string();
            for i in 0..16 {
                text.push_str(&format!("\x1b[38;5;{i}m\u{2588}\u{2588}"));
            }
            text.push_str("\x1b[0m\r\n\u{2588}\u{2588}\r\n");
            for i in 0..16 {
                text.push_str(&format!("\x1b[48;5;{i}m  "));
            }
            text.push_str("\x1b[0m\r\nHHHH\x1b[6;1H\x1b[?25h\x1b[2 q");
            raw.feed(text.as_bytes());
            *crate::link::lock(link.terminal()) = Some(crate::link::Shown::Raw(Box::new(raw)));
            view.rows = Default::default();
            window.activate_window();
            window.focus(&view.input.focus);
            cx.notify();
        })
        .unwrap();
    let mut samples = Vec::new();
    for (name, glass, remote) in [
        ("colors-solid", false, false),
        ("colors-glass", true, false),
        ("colors-remote", false, true),
    ] {
        handle
            .update(cx, |view, window, cx| {
                view.appearance.settings.glass = glass;
                view.appearance.settings.glass_darkness = 50;
                if remote {
                    show_remote(view);
                }
                view.rows = Default::default();
                window.refresh();
                cx.notify();
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(400))
            .await;
        super::appearance_native_e2e::snapshot(capture, evidence, name);
        handle.update(cx, |view, window, _| {
            let grid = view.input.grid.unwrap();
            let (x,y,w,h) = (f32::from(grid.origin.x),f32::from(grid.origin.y),f32::from(grid.cell_width),f32::from(grid.line_height));
            let image = image::open(evidence.join(format!("{name}.png"))).unwrap().to_rgb8();
            let scale = image.width() as f32 / 960.;
            assert_eq!(scale, window.scale_factor());
            assert!(scale == 1. || scale == 2.);
            let case = std::env::var("COLOR_QA").unwrap();
            let defaults = matches!(case.as_str(), "none" | "unreadable" | "malformed" | "partial");
            let mut ansi = if defaults { fixture::DEFAULTS } else { fixture::ANSI };
            if case == "partial" { ansi[1] = fixture::ANSI[1]; }
            if case == "ghostty-named" { ansi[0] = 0x000080; ansi[1] = 0xbb4455; }
            let fg = if case == "ghostty-named" { 0xadd8e6 } else if defaults { 0xd4d4d4 } else { fixture::FG };
            let bg = if case == "ghostty-named" { 0x1a1a1a } else if defaults { 0x1b1b1b } else { fixture::BG };
            let cursor = if case == "ghostty-named" { 0xffd700 } else if defaults { 0xd4d4d4 } else { fixture::CURSOR };
            let mut pixels = Vec::new();
            let mut check = |col: f32, row: f32, color: u32, label: &str| {
                let actual = image.get_pixel(((x+w*col)*scale).floor() as u32, ((y+h*row)*scale).floor() as u32).0;
                let expected = [(color>>16) as u8, (color>>8) as u8, color as u8];
                assert!(actual.iter().zip(expected).all(|(a,b)| a.abs_diff(b) <= 3), "{name} {label}: actual {actual:?}, expected {expected:?}");
                pixels.push(serde_json::json!({"label":label,"column":col,"row":row,"actual":actual,"expected":expected}));
            };
            for (i,color) in ansi.iter().enumerate() {
                check(i as f32*2.+0.5,0.5,*color,"ANSI foreground");
                check(i as f32*2.+0.5,2.5,*color,"ANSI background");
            }
            check(0.5,1.5,fg,"foreground");
            let background = if glass {
                if defaults { 0 } else if case == "ghostty-named" { 0x0d0d0d } else { 0x101820 }
            } else { bg };
            check(36.5,1.5,background,"background");
            check(0.05,5.5,cursor,"cursor");
            samples.push(serde_json::json!({"capture":name,"name":format!("{name} capitals"),"box":[x,y+3.*h,w*4.,h],"probe":[x,w*4.],"color":[(fg>>16)&255,(fg>>8)&255,fg&255]}));
            fs::write(evidence.join(format!("{name}-grid.json")), serde_json::to_vec_pretty(&serde_json::json!({"x":x,"y":y,"w":w,"h":h,"scale":scale,"checks":35,"pixels":pixels})).unwrap()).unwrap();
            println!("{case} {name}: 35 color pixel checks at {scale}x passed");
        }).unwrap();
    }
    fs::write(
        evidence.join("color-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
}

fn show_remote(view: &mut SeerWindow) {
    let Content::Terminal(link, _) = &mut view.content else {
        panic!("missing terminal")
    };
    let frame = {
        let raw = crate::link::lock(link.terminal());
        let raw = raw.as_ref().unwrap();
        seer_core::TerminalFrame {
            rows: (0..raw.rows()).map(|i| raw.row(i)).collect(),
            cursor: raw.cursor(),
            modes: Default::default(),
        }
    };
    let mut person = link.people.people[0].clone();
    person.user_id = "remote".into();
    person.name = "Remote".into();
    link.people.people.push(person);
    link.select_person("remote").unwrap();
    link.people.pane = Some("remote-pane".into());
    link.receive(crate::link::Event::Room(
        crate::room::transport::RoomEvent::Message(seer_core::proto::ServerMsg::Cells {
            user: "remote".into(),
            pane: "remote-pane".into(),
            seq: 1,
            frame,
        }),
    ))
    .unwrap();
    assert!(link.remote());
}

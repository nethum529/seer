use std::{fs, path::Path, time::Duration};

use gpui::{WindowHandle, px};

use super::{Content, SeerWindow};

pub(super) fn prepare(root: &Path) {
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    // SAFETY: the native fixture sets its environment before GPUI or worker threads start.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_DIRS", root.join("system-config"));
        std::env::remove_var("KITTY_CONFIG_DIRECTORY");
    };
    let Ok(case) = std::env::var("FONT_QA") else {
        return;
    };
    let config = root.join("config");
    let family = std::env::var("FONT_QA_FAMILY").unwrap_or("Menlo".into());
    let size = std::env::var("FONT_QA_SIZE").unwrap_or("16".into());
    let ghostty = format!("font-family = \"{family}\"\nfont-size = {size}\n");
    let kitty = format!("font_family family=\"{family}\"\nfont_size {size}\n");
    let alacritty = format!("[font]\nsize = {size}\nnormal = {{ family = '{family}' }}\n");
    match case.as_str() {
        "ghostty" | "precedence" => write(&config.join("ghostty/config"), &ghostty),
        "ghostty-macos" => write(
            &home.join("Library/Application Support/com.mitchellh.ghostty/config.ghostty"),
            &ghostty,
        ),
        "ghostty-includes" => {
            write(
                &config.join("ghostty/config"),
                "font-family = Courier\nfont-size = 10\nconfig-file = font.conf\nfont-size = 11\n",
            );
            write(
                &config.join("ghostty/font.conf"),
                &format!("font-family = \"\"\n{ghostty}"),
            );
        }
        "kitty-includes" => {
            write(
                &config.join("kitty/kitty.conf"),
                "font_family Courier\ninclude font.conf\n",
            );
            write(&config.join("kitty/font.conf"), &kitty);
        }
        "alacritty-imports" => {
            write(
                &config.join("alacritty/alacritty.toml"),
                "[general]\nimport = ['font.toml']\n",
            );
            write(&config.join("alacritty/font.toml"), &alacritty);
        }
        "kitty-continuation" => write(
            &config.join("kitty/kitty.conf"),
            include_str!("../../tests/fixtures/kitty-continuation.conf"),
        ),
        "kitty-missing-include" | "kitty-unreadable-include" => {
            write(
                &config.join("kitty/kitty.conf"),
                include_str!("../../tests/fixtures/kitty-missing-include.conf"),
            );
            if case == "kitty-unreadable-include" {
                fs::create_dir_all(config.join("kitty/optional-machine.conf")).unwrap();
            }
        }
        "kitty-unreadable" => fs::create_dir_all(config.join("kitty/kitty.conf")).unwrap(),
        "kitty" => write(&config.join("kitty/kitty.conf"), &kitty),
        "alacritty" => write(&config.join("alacritty/alacritty.toml"), &alacritty),
        "iterm2" | "iterm2-binary" => {
            let path = home.join("Library/Preferences/com.googlecode.iterm2.plist");
            write(
                &path,
                &format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict><key>Default Bookmark Guid</key><string>chosen</string><key>New Bookmarks</key><array><dict><key>Guid</key><string>other</string><key>Normal Font</key><string>Courier 10</string></dict><dict><key>Guid</key><string>chosen</string><key>Normal Font</key><string>{family}-Regular {size} {{&quot;featureSettings&quot;:[]}}</string></dict></array></dict></plist>"
                ),
            );
            if case == "iterm2-binary" {
                assert!(
                    std::process::Command::new("/usr/bin/plutil")
                        .args(["-convert", "binary1"])
                        .arg(path)
                        .status()
                        .unwrap()
                        .success()
                );
            }
        }
        "missing-font" => write(
            &config.join("ghostty/config"),
            "font-family = SeerMissingFont570\nfont-size = 24\n",
        ),
        "empty" => write(&config.join("ghostty/config"), "background = 123456\n"),
        "unreadable" => fs::create_dir_all(config.join("ghostty/config")).unwrap(),
        "malformed" => write(
            &config.join("alacritty/alacritty.toml"),
            "[font\nsize = no\n",
        ),
        "invalid-size" => write(
            &config.join("ghostty/config"),
            "font-family = JetBrains Mono\nfont-size = nan\n",
        ),
        "none" => {}
        _ => panic!("unknown font QA case"),
    }
    if case == "precedence" {
        write(
            &config.join("kitty/kitty.conf"),
            "font_family Courier\nfont_size 10\n",
        );
        write(
            &config.join("alacritty/alacritty.toml"),
            "[font]\nsize = 11\nnormal.family = 'Courier'\n",
        );
    }
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let family = std::env::var("FONT_QA_EXPECT_FAMILY")
        .or_else(|_| std::env::var("FONT_QA_FAMILY"))
        .unwrap_or("Menlo".into());
    let size: f32 = std::env::var("FONT_QA_EXPECT_SIZE")
        .or_else(|_| std::env::var("FONT_QA_SIZE"))
        .unwrap_or("16".into())
        .parse()
        .unwrap();
    handle.update(cx, |view, window, cx| {
        assert_eq!(view.font.family.as_ref(), family, "the window must use the configured family");
        let grid = view.input.grid.unwrap();
        let text = window.text_system();
        let id = text.resolve_font(&gpui::font(family.clone()));
        let expected_width = text.advance(id, px(size), 'm').unwrap().width;
        assert!((f32::from(grid.cell_width - expected_width)).abs() < 0.01, "the window must use the configured size");
        view._messages = None;
        let Content::Terminal(link, _) = &view.content else { panic!("missing terminal") };
        let mut raw = seer_runtime::PaneGrid::new(80, 24);
        raw.feed("\x1b[2J\x1b[H\x1b[?25l\r\n  HHHH HHHH\r\n  iiiimmmm\r\n  00001111\r\n  \u{250c}\u{2500}\u{252c}\u{2500}\u{2510}\r\n  \u{2502} \u{2502} \u{2502}\r\n  \u{2514}\u{2500}\u{2534}\u{2500}\u{2518}\r\n".as_bytes());
        *crate::link::lock(link.terminal()) = Some(crate::link::Shown::Raw(Box::new(raw)));
        view.rows = Default::default();
        window.activate_window();
        cx.notify();
        let x = f32::from(grid.origin.x + grid.cell_width * 2.);
        let y = f32::from(grid.origin.y + grid.line_height);
        let w = f32::from(grid.cell_width);
        let h = f32::from(grid.line_height);
        fs::write(evidence.join("font-samples.json"), serde_json::to_vec_pretty(&serde_json::json!([{
            "capture": "font", "name": "Terminal capitals", "box": [x, y, w * 4., h], "probe": [x, w * 4.]
        }])).unwrap()).unwrap();
        fs::write(evidence.join("font-grid.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "family": family, "size": size, "x": x, "y": y, "width": w, "height": h, "scale": window.scale_factor()
        })).unwrap()).unwrap();
    }).unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(350))
        .await;
    super::appearance_native_e2e::snapshot(capture, evidence, "font");
    handle
        .update(cx, |view, _, cx| {
            let Content::Terminal(link, _) = &mut view.content else {
                panic!("missing terminal")
            };
            let frame = {
                let raw = crate::link::lock(link.terminal());
                let raw = raw.as_ref().unwrap();
                seer_core::TerminalFrame {
                    rows: (0..raw.rows()).map(|index| raw.row(index)).collect(),
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
            assert_eq!(view.font.family.as_ref(), family);
            view.rows = Default::default();
            cx.notify();
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(350))
        .await;
    super::appearance_native_e2e::snapshot(capture, evidence, "font-remote");
    let mut samples: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(evidence.join("font-samples.json")).unwrap()).unwrap();
    let mut remote = samples[0].clone();
    remote["capture"] = "font-remote".into();
    remote["name"] = "Remote terminal capitals".into();
    samples.push(remote);
    fs::write(
        evidence.join("font-samples.json"),
        serde_json::to_vec_pretty(&samples).unwrap(),
    )
    .unwrap();
}

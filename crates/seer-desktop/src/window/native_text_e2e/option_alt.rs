use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::window::SeerWindow;
use gpui::WindowHandle;

pub(super) fn prepare(root: &Path) {
    let Ok(case) = std::env::var("OPTION_QA") else {
        return;
    };
    // SAFETY: prepare runs before GPUI or worker threads start.
    unsafe {
        std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let (app, side) = case.split_once(':').unwrap();
    let config = root.join("config");
    let home = root.join("home");
    let value = match side {
        "both" => "true",
        "off" => "false",
        other => other,
    };
    let (path, contents) = match app {
        "ghostty" => (
            config.join("ghostty/config"),
            format!("macos-option-as-alt = {value}\n"),
        ),
        "ghostty-includes" => {
            write(
                &config.join("ghostty/keys.conf"),
                &format!("macos-option-as-alt = {value}\n"),
            );
            (
                config.join("ghostty/config"),
                "macos-option-as-alt = false\nconfig-file = keys.conf\n".into(),
            )
        }
        "kitty" | "kitty-includes" => {
            let value = if side == "off" { "no" } else { side };
            if app == "kitty-includes" {
                write(
                    &config.join("kitty/keys.conf"),
                    &format!("macos_option_as_alt {value}\n"),
                );
                (
                    config.join("kitty/kitty.conf"),
                    "macos_option_as_alt no\ninclude keys.conf\n".into(),
                )
            } else {
                (
                    config.join("kitty/kitty.conf"),
                    format!("macos_option_as_alt {value}\n"),
                )
            }
        }
        "alacritty" | "alacritty-imports" => {
            let value = match side {
                "both" => "Both",
                "left" => "OnlyLeft",
                "right" => "OnlyRight",
                _ => "None",
            };
            let contents = format!("[window]\noption_as_alt = '{value}'\n");
            if app == "alacritty-imports" {
                write(&config.join("alacritty/keys.toml"), &contents);
                (
                    config.join("alacritty/alacritty.toml"),
                    "[general]\nimport = ['keys.toml']\n".into(),
                )
            } else {
                (config.join("alacritty/alacritty.toml"), contents)
            }
        }
        "iterm2" | "iterm2-binary" | "iterm2-left-fallback" => {
            iterm(&home, app, side);
            return;
        }
        "unreadable" => {
            fs::create_dir_all(config.join("ghostty/config")).unwrap();
            return;
        }
        "empty" | "precedence" => (config.join("ghostty/config"), "font-size = 12\n".into()),
        "none" => return,
        _ => panic!("unknown Option QA app"),
    };
    write(&path, &contents);
    if app == "precedence" {
        write(
            &config.join("kitty/kitty.conf"),
            "macos_option_as_alt both\n",
        );
    }
}

fn iterm(home: &Path, app: &str, side: &str) {
    let mut chosen = plist::Dictionary::new();
    chosen.insert("Guid".into(), "chosen".into());
    chosen.insert(
        "Option Key Sends".into(),
        (if matches!(side, "both" | "left") {
            2i64
        } else {
            0
        })
        .into(),
    );
    if app != "iterm2-left-fallback" {
        chosen.insert(
            "Right Option Key Sends".into(),
            (if matches!(side, "both" | "right") {
                2i64
            } else {
                0
            })
            .into(),
        );
    }
    let mut root = plist::Dictionary::new();
    root.insert("Default Bookmark Guid".into(), "chosen".into());
    root.insert(
        "New Bookmarks".into(),
        plist::Value::Array(vec![
            plist::Value::Dictionary(Default::default()),
            plist::Value::Dictionary(chosen),
        ]),
    );
    let path = home.join("Library/Preferences/com.googlecode.iterm2.plist");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let root = plist::Value::Dictionary(root);
    if app == "iterm2-binary" {
        root.to_file_binary(path).unwrap();
    } else {
        root.to_file_xml(path).unwrap();
    }
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

pub(super) fn shell(root: &Path, evidence: &Path) -> PathBuf {
    if std::env::var_os("OPTION_QA").is_none() {
        return "/bin/sh".into();
    }
    fs::write(evidence.join("pty.bin"), []).unwrap();
    let path = root.join("option-shell");
    write(
        &path,
        &format!(
            "#!/bin/sh\nexec /usr/bin/python3 -u -c 'import os,tty\ntty.setraw(0)\nf=open(\"{}/pty.bin\",\"ab\",buffering=0)\nos.write(1,b\"Ready579\")\nwhile True:\n b=os.read(0,4096)\n f.write(b)\n if b == b\"|\": os.write(1,b\"\\x1b[=1uKitty579\")\n'\n",
            evidence.display()
        ),
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |view, window, _| {
            window.activate_window();
            window.focus(&view.input.focus);
        })
        .unwrap();
    pause(cx).await;
    let case = std::env::var("OPTION_QA").unwrap();
    let (app, side) = case.split_once(':').unwrap();
    let side = if app == "iterm2-left-fallback" {
        "both"
    } else {
        side
    };
    let input = evidence.join("pty.bin");
    let mut expected = Vec::new();
    let mut rows = Vec::new();
    for kitty in [false, true] {
        if kitty {
            post(handle, 0x2a, 1 << 17, cx);
            expected.push(b'|');
            pause(cx).await;
        }
        for (name, flag) in [("left", 0x20), ("right", 0x40)] {
            let alt = side == "both" || side == name;
            for (label, key, extra, legacy, negotiated, native) in [
                ("B", 0x0b, 0, "\x1bb", "\x1b[98;3u", "\u{222b}"),
                ("Shift B", 0x0b, 1 << 17, "\x1bB", "\x1b[98;4u", "\u{131}"),
                ("Control B", 0x0b, 1 << 18, "\x1b\x02", "\x1b[98;7u", "\x02"),
                ("Space", 0x31, 0, "\x1b ", "\x1b[32;3u", "\u{a0}"),
                (
                    "Shift slash",
                    0x2c,
                    1 << 17,
                    "\x1b?",
                    "\x1b[47;4u",
                    "\u{bf}",
                ),
                ("Enter", 0x24, 0, "\x1b\r", "\x1b[13;3u", "\r"),
                ("Keypad Enter", 0x4c, 0, "\x1b\x03", "\x1b[57414;3u", "\x03"),
                ("Backspace", 0x33, 0, "\x1b\x7f", "\x1b\x7f", "\x1b\x7f"),
            ] {
                post(handle, key, (1 << 19) | flag | extra, cx);
                pause(cx).await;
                let bytes = if alt {
                    if kitty { negotiated } else { legacy }
                } else if kitty && label == "Control B" {
                    "\x1b[98;5u"
                } else {
                    native
                };
                expected.extend_from_slice(bytes.as_bytes());
                let actual = fs::read(&input).unwrap_or_default();
                rows.push(format!(
                    "kitty={kitty} {name} {label}: {}",
                    hex(&actual[actual.len().min(expected.len() - bytes.len())..])
                ));
                fs::write(evidence.join("keys.txt"), rows.join("\n")).unwrap();
                assert_eq!(actual, expected, "{case} kitty={kitty} {name} {label}");
            }
            if !alt {
                post(handle, 0x0e, (1 << 19) | flag, cx);
                pause(cx).await;
                post(handle, 0x0e, 0, cx);
                pause(cx).await;
                expected.extend_from_slice("\u{e9}".as_bytes());
                assert_eq!(
                    fs::read(&input).unwrap(),
                    expected,
                    "native composition {case} {name}"
                );
            }
        }
    }
    fs::write(
        evidence.join("passed.txt"),
        format!("{case}\n{}\n", hex(&expected)),
    )
    .unwrap();
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn post(handle: WindowHandle<SeerWindow>, key: u16, flags: u64, cx: &mut gpui::AsyncApp) {
    handle
        .update(cx, |_, _, _| {
            super::super::sidebar_native_e2e::post_shortcut(key, flags)
        })
        .unwrap();
}

async fn pause(cx: &mut gpui::AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(130))
        .await;
}

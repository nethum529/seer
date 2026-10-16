use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use gpui::{TestAppContext, point, px};

use super::input_e2e::{Runtime, open_view};

#[gpui::test]
fn keyboard_push_bursts_keep_the_terminal_live(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    cx.simulate_click(point(px(300.), px(100.)), gpui::Modifiers::default());
    thread::sleep(Duration::from_millis(30));
    cx.run_until_parked();
    for (prefix, marker) in [
        ("", "PrimaryAlive"),
        ("\x1b[?1049h", "AlternateAlive"),
        ("\x1b[?1049l\x1bc", "ResetAlive"),
        ("\x1b[?2026h", "SyncAlive"),
    ] {
        cx.simulate_input(&format!(
            "{prefix}{}\x1b[?2026l\r\n{marker}\r\n",
            "\x1b[>1u".repeat(4097)
        ));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            cx.run_until_parked();
            let visible = view.read_with(cx, |view, _| {
                view.with_raw(|grid| {
                    grid.snapshot().rows.iter().any(|row| {
                        row.iter()
                            .map(|cell| cell.character)
                            .collect::<String>()
                            .contains(marker)
                    })
                })
                .unwrap_or(false)
            });
            if visible {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "terminal output stopped before {marker}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[gpui::test]
fn edit_keys_match_alacritty_with_and_without_negotiation(cx: &mut TestAppContext) {
    for extended in [false, true] {
        let runtime = Runtime::start_with_output(if extended {
            "\x1b[=5uReady\r\n"
        } else {
            "Ready\r\n"
        });
        let (_, cx) = open_view(&runtime, cx);
        cx.simulate_click(point(px(300.), px(100.)), gpui::Modifiers::default());
        thread::sleep(Duration::from_millis(30));
        cx.run_until_parked();
        for (key, legacy, negotiated) in edit_keys() {
            let before = runtime.input().len();
            cx.simulate_keystrokes(&format!("{key} z"));
            let deadline = Instant::now() + Duration::from_secs(2);
            while (runtime.input().len() <= before || !runtime.input().ends_with(b"z"))
                && Instant::now() < deadline
            {
                cx.run_until_parked();
                thread::sleep(Duration::from_millis(10));
            }
            let expected = if extended { negotiated } else { legacy };
            let received = runtime.input()[before..].to_vec();
            println!("extended={extended} {key}: {received:02x?}");
            assert_eq!(
                received,
                [expected.as_bytes(), b"z"].concat(),
                "{key}, extended={extended}"
            );
        }
    }
}

fn edit_keys() -> Vec<(&'static str, &'static str, &'static str)> {
    let mac = cfg!(target_os = "macos");
    vec![
        (
            "ctrl-backspace",
            if mac { "\x7f" } else { "\x08" },
            "\x1b[127;5u",
        ),
        ("ctrl-delete", "\x1b[3;5~", "\x1b[3;5~"),
        ("ctrl-left", "\x1b[1;5D", "\x1b[1;5D"),
        ("ctrl-right", "\x1b[1;5C", "\x1b[1;5C"),
        ("ctrl-up", "\x1b[1;5A", "\x1b[1;5A"),
        ("ctrl-down", "\x1b[1;5B", "\x1b[1;5B"),
        ("ctrl-home", "\x1b[1;5H", "\x1b[1;5H"),
        ("ctrl-end", "\x1b[1;5F", "\x1b[1;5F"),
        ("ctrl-enter", "\r", "\x1b[13;5u"),
        ("ctrl-tab", "\t", "\x1b[9;5u"),
        (
            "alt-backspace",
            "\x1b\x7f",
            if mac { "\x1b\x7f" } else { "\x1b[127;3u" },
        ),
        ("alt-delete", "\x1b[3;3~", "\x1b[3;3~"),
        ("alt-left", "\x1b[1;3D", "\x1b[1;3D"),
        ("alt-right", "\x1b[1;3C", "\x1b[1;3C"),
        ("alt-up", "\x1b[1;3A", "\x1b[1;3A"),
        ("alt-down", "\x1b[1;3B", "\x1b[1;3B"),
        ("alt-home", "\x1b[1;3H", "\x1b[1;3H"),
        ("alt-end", "\x1b[1;3F", "\x1b[1;3F"),
        (
            "alt-enter",
            if mac { "\r" } else { "\x1b\r" },
            if mac { "\r" } else { "\x1b[13;3u" },
        ),
        (
            "alt-tab",
            if mac { "\t" } else { "\x1b\t" },
            if mac { "\t" } else { "\x1b[9;3u" },
        ),
        ("shift-backspace", "\x7f", "\x1b[127;2u"),
        ("shift-delete", "\x1b[3;2~", "\x1b[3;2~"),
        ("shift-left", "\x1b[1;2D", "\x1b[1;2D"),
        ("shift-right", "\x1b[1;2C", "\x1b[1;2C"),
        ("shift-up", "\x1b[1;2A", "\x1b[1;2A"),
        ("shift-down", "\x1b[1;2B", "\x1b[1;2B"),
        ("shift-home", "", ""),
        ("shift-end", "", ""),
        ("shift-enter", "\r", "\x1b[13;2u"),
        ("shift-tab", "\x1b[Z", "\x1b[9;2u"),
    ]
}

#[gpui::test]
fn fish_edit_keys_delete_the_previous_word(cx: &mut TestAppContext) {
    let Some(fish) = std::env::var_os("SEER_TEST_FISH").map(PathBuf::from) else {
        eprintln!("Set SEER_TEST_FISH to run the fish 4 shell contract");
        return;
    };
    let directory = tempfile::Builder::new()
        .prefix("s500")
        .tempdir_in("/tmp")
        .unwrap();
    let shell = directory.path().join("shell");
    let result = directory.path().join("result");
    fs::write(directory.path().join("input"), []).unwrap();
    fs::write(&shell, format!(
        "#!/bin/sh\nexec '{}' --no-config --interactive --init-command 'function fish_prompt; printf FishReady; end; bind enter \"commandline > {}; commandline -f cancel-commandline\"'\n",
        fish.display(), result.display()
    )).unwrap();
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700)).unwrap();
    let runtime = Runtime::start_in(directory, shell);
    let (view, cx) = open_view(&runtime, cx);
    cx.simulate_click(point(px(300.), px(100.)), gpui::Modifiers::default());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        let ready = view.read_with(cx, |view, _| {
            view.with_raw(|grid| {
                grid.snapshot().rows.iter().any(|row| {
                    row.iter()
                        .map(|cell| cell.character)
                        .collect::<String>()
                        .contains("FishReady")
                })
            })
            .unwrap_or(false)
        });
        if ready {
            break;
        }
        assert!(Instant::now() < deadline, "fish must show its prompt");
        thread::sleep(Duration::from_millis(10));
    }
    cx.simulate_input("one two");
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "alt-backspace->\x7f enter"
    } else {
        "ctrl-backspace enter"
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while fs::read_to_string(&result).unwrap_or_default() != "one \n" && Instant::now() < deadline {
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fs::read_to_string(result).unwrap(), "one \n");
}

use gpui::{Modifiers, TestAppContext, VisualTestContext, point, px, size};
use std::time::Duration;

use super::input_e2e::{Runtime, open_view};
use super::watch_e2e::pump;

fn toggle() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-b"
    } else {
        "ctrl-shift-b"
    }
}

pub(super) fn focus_shortcut() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-shift-b"
    } else {
        "ctrl-alt-shift-b"
    }
}

fn settle(cx: &mut VisualTestContext) {
    std::thread::sleep(Duration::from_millis(320));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear();
    });
    pump(cx);
}

#[gpui::test]
fn sidebar_toggle_preserves_selection_focus_and_hidden_access(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    std::fs::write(
        runtime.directory.path().join("settings.toml"),
        "glass = false\nhide_top_bar = false\n",
    )
    .unwrap();
    let (view, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    settle(cx);
    let before = cx.debug_bounds("terminal").unwrap();
    let pane = view.read_with(cx, |view, _| {
        let super::Content::Terminal(link, _) = &view.content else {
            panic!()
        };
        link.shown().unwrap().to_owned()
    });
    cx.simulate_keystrokes(toggle());
    settle(cx);
    let hidden = cx.debug_bounds("terminal").unwrap();
    assert_eq!(hidden.left(), px(0.));
    assert_eq!(hidden.size.width - before.size.width, px(248.));
    cx.update(|window, cx| assert!(view.read(cx).input.focus.is_focused(window)));
    cx.simulate_keystrokes(toggle());
    settle(cx);
    assert_eq!(cx.debug_bounds("terminal").unwrap(), before);
    cx.update(|window, cx| assert!(view.read(cx).keys.tabs.is_focused(window)));
    view.read_with(cx, |view, _| {
        let super::Content::Terminal(link, _) = &view.content else {
            panic!()
        };
        assert_eq!(link.shown(), Some(pane.as_str()));
    });
    cx.simulate_keystrokes(toggle());
    settle(cx);
    cx.simulate_keystrokes(focus_shortcut());
    settle(cx);
    assert_eq!(cx.debug_bounds("terminal").unwrap(), before);
    cx.simulate_keystrokes("shift-tab enter");
    pump(cx);
    assert!(view.read_with(cx, |view, _| view.people_menu.is_some()));
    cx.simulate_keystrokes("escape");
    cx.simulate_click(point(px(400.), px(200.)), Modifiers::default());
    cx.simulate_keystrokes(toggle());
    settle(cx);
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-n"
    } else {
        "ctrl-shift-n"
    });
    pump(cx);
    view.read_with(cx, |view, _| {
        let super::Content::Terminal(link, _) = &view.content else {
            panic!()
        };
        assert_eq!(link.terminals().len(), 2);
    });
}

#[gpui::test]
fn sidebar_hidden_state_survives_a_new_window(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    std::fs::write(
        runtime.directory.path().join("settings.toml"),
        "glass = false\nhide_top_bar = false\n",
    )
    .unwrap();
    {
        let (_, cx) = open_view(&runtime, cx);
        cx.simulate_resize(size(px(960.), px(640.)));
        cx.simulate_keystrokes(toggle());
        settle(cx);
        assert_eq!(cx.debug_bounds("terminal").unwrap().left(), px(0.));
    }
    let (_, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    settle(cx);
    assert_eq!(cx.debug_bounds("terminal").unwrap().left(), px(0.));
}

#[gpui::test]
fn sidebar_motion_resizes_the_pty_only_when_it_settles(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::Builder::new()
        .prefix("s514")
        .tempdir_in("/tmp")
        .unwrap();
    let script = directory.path().join("shell");
    let events = directory.path().join("sizes");
    std::fs::write(&script, format!(
        "#!/bin/sh\nexec python3 -u -c 'import os,signal,tty\ntty.setraw(0)\nf=open(\"{}\",\"a\",buffering=1)\ndef resized(*args):\n s=os.get_terminal_size(0)\n f.write(str(s.columns)+\" \"+str(s.lines)+\"\\n\")\nsignal.signal(signal.SIGWINCH,resized)\nopen(\"{}/input\",\"wb\").close()\nwhile True: os.read(0,4096)'\n",
        events.display(), directory.path().display()
    )).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let runtime = Runtime::start_in(directory, script);
    std::fs::write(
        runtime.directory.path().join("settings.toml"),
        "glass = false\nhide_top_bar = false\n",
    )
    .unwrap();
    let (_, cx) = open_view(&runtime, cx);
    cx.simulate_resize(size(px(960.), px(640.)));
    settle(cx);
    let count = || {
        std::fs::read_to_string(&events)
            .unwrap_or_default()
            .lines()
            .count()
    };
    let initial = count();
    cx.simulate_keystrokes(toggle());
    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(15));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
        assert_eq!(
            count(),
            initial,
            "the PTY must keep its grid during hide motion"
        );
    }
    cx.simulate_keystrokes(toggle());
    settle(cx);
    assert_eq!(
        count(),
        initial,
        "reversing to the original width must not resize the PTY"
    );
    cx.simulate_keystrokes(toggle());
    settle(cx);
    assert_eq!(count(), initial + 1, "hide must resize once");
    cx.simulate_keystrokes(toggle());
    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(15));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
        assert_eq!(
            count(),
            initial + 1,
            "the PTY must keep its grid during show motion"
        );
    }
    settle(cx);
    assert_eq!(count(), initial + 2, "show must resize once");
}

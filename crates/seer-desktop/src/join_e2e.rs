use std::process::Command;

use gpui::{ClipboardItem, TestAppContext};

use crate::window::SeerWindow;

#[gpui::test]
fn join_accepts_invite_lines_and_selects_the_system_name(cx: &mut TestAppContext) {
    if std::env::var_os("SEER_JOIN_E2E_CHILD").is_none() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let config = directory.path().join("config");
        let state = directory.path().join("state");
        for path in [&home, &config, &state] {
            std::fs::create_dir(path).unwrap();
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "join_e2e::join_accepts_invite_lines_and_selects_the_system_name",
                "--nocapture",
            ])
            .env("SEER_JOIN_E2E_CHILD", "1")
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_STATE_HOME", &state)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for path in [&home, &config, &state] {
            assert_eq!(std::fs::read_dir(path).unwrap().count(), 0);
        }
        return;
    }
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::assets::load_fonts(cx).unwrap();
        SeerWindow::new(window, cx)
    });
    cx.run_until_parked();
    let join = cx.debug_bounds("start-join").unwrap();
    cx.simulate_click(join.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("join-field").is_some());
    cx.simulate_input("bad invite");
    cx.simulate_keystrokes("enter");
    assert!(cx.debug_bounds("join-error").is_some());
    let capsule = format!("SEER2-{}-seat", "a".repeat(64));
    for line in [
        capsule.clone(),
        format!("seer join {capsule}"),
        format!("curl -fsSL https://example.com/install.sh | sh -s -- {capsule}"),
    ] {
        cx.write_to_clipboard(ClipboardItem::new_string(line));
        cx.simulate_keystrokes("secondary-a secondary-v enter");
        assert!(cx.debug_bounds("join-back").is_some());
        cx.simulate_keystrokes("secondary-c");
        let name = std::env::var("USER")
            .ok()
            .filter(|name| !name.is_empty())
            .or_else(|| std::env::var("LOGNAME").ok());
        assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), name);
        cx.simulate_input("Friend");
        cx.simulate_keystrokes("secondary-a secondary-c");
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("Friend")
        );
        let back = cx.debug_bounds("join-back").unwrap();
        cx.simulate_click(back.center(), gpui::Modifiers::default());
        cx.simulate_keystrokes("secondary-a secondary-c");
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("")
        );
    }
}

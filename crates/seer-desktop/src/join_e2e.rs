use gpui::{ClipboardItem, TestAppContext};

use crate::window::SeerWindow;

#[gpui::test]
fn join_accepts_invite_lines_and_selects_the_system_name(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::assets::load_fonts(cx).unwrap();
        SeerWindow::new(window, cx)
    });
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

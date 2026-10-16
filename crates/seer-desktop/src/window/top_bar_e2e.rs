use gpui::{Modifiers, TestAppContext, VisualTestContext, point, px, size};

use super::input_e2e::{Runtime, open_view};
use super::watch_e2e::pump;

#[gpui::test]
fn hover_slides_content_without_resizing_the_shell(cx: &mut TestAppContext) {
    let runtime = Runtime::start_shell();
    std::fs::write(
        runtime.directory.path().join("settings.toml"),
        "hide_lines = true\nhide_top_bar = true\n",
    )
    .unwrap();
    let (_, cx) = open_view(&runtime, cx);
    for width in [960., 600.] {
        cx.simulate_resize(size(px(width), px(640.)));
        cx.simulate_mouse_move(point(px(400.), px(200.)), None, Modifiers::default());
        settle(cx);
        let hidden = cx.debug_bounds("terminal").unwrap();
        assert!(
            hidden.size.height > px(500.),
            "the terminal must remain visible after a window resize"
        );
        assert!(
            hidden.top() < px(if width < 720. { 50. } else { 44. }),
            "content must fill the hidden bar"
        );
        cx.simulate_click(hidden.center(), Modifiers::default());
        let before = runtime.directory.path().join("before");
        cx.simulate_input(&format!("stty size > {}", before.display()));
        cx.simulate_keystrokes("enter");
        pump(cx);
        cx.simulate_mouse_move(point(px(400.), px(10.)), None, Modifiers::default());
        settle(cx);
        let shown = cx.debug_bounds("terminal").unwrap();
        assert_eq!(shown.top() - hidden.top(), px(44.));
        assert_eq!(shown.size, hidden.size);
        let after = runtime.directory.path().join("after");
        cx.simulate_input(&format!("stty size > {}", after.display()));
        cx.simulate_keystrokes("enter");
        pump(cx);
        assert_eq!(
            std::fs::read(&before).unwrap(),
            std::fs::read(after).unwrap()
        );
        cx.simulate_mouse_move(point(px(100.), px(62.)), None, Modifiers::default());
        settle(cx);
        assert_eq!(
            cx.debug_bounds("terminal").unwrap(),
            shown,
            "shifted top-row controls must stay under the pointer"
        );
        cx.simulate_mouse_move(point(px(400.), px(200.)), None, Modifiers::default());
        settle(cx);
        assert_eq!(cx.debug_bounds("terminal").unwrap(), hidden);
    }
}

fn settle(cx: &mut VisualTestContext) {
    std::thread::sleep(std::time::Duration::from_millis(320));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear();
    });
    pump(cx);
}

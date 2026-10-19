use super::{Content, SeerWindow, sidebar_native_e2e::post_shortcut};
use gpui::WindowHandle;
use std::{fs, path::Path, time::Duration};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.update(|cx| cx.activate(true)).unwrap();
    let mut results = Vec::new();
    for (key, flags, count, selected) in [
        (0x2d, 1 << 20, 2, 1),
        (0x2d, 1 << 20, 3, 2),
        (0x7c, 1 << 20, 3, 0),
        (0x7b, 1 << 20, 3, 2),
        (0x12, 1 << 20, 3, 0),
        (0x1a, 1 << 20, 3, 0),
    ] {
        handle
            .update(cx, |_, _, _| post_shortcut(key, flags))
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(300))
            .await;
        handle
            .update(cx, |view, window, _| {
                let Content::Terminal(link, _) = &view.content else {
                    panic!("terminal required")
                };
                assert_eq!(link.terminals().len(), count);
                let at = link
                    .terminals()
                    .iter()
                    .position(|terminal| Some(terminal.pane.as_str()) == link.shown())
                    .unwrap();
                assert_eq!(
                    at, selected,
                    "native key {key:#x} must select in sidebar order"
                );
                assert!(view.input.focus.is_focused(window));
                results.push(
                    serde_json::json!({"key": key, "flags": flags, "count": count, "selected": at}),
                );
            })
            .unwrap();
    }
    fs::write(
        evidence.join("native-switch.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
    handle
        .update(cx, |view, _, cx| {
            let Content::Terminal(link, _) = &view.content else {
                panic!("terminal required")
            };
            let panes: Vec<_> = link
                .terminals()
                .iter()
                .skip(1)
                .map(|terminal| terminal.pane.clone())
                .collect();
            for pane in panes {
                view.send_close(&pane, cx);
            }
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(200))
        .await;
}

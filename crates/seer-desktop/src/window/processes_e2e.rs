use std::{
    thread,
    time::{Duration, Instant},
};

use gpui::{TestAppContext, VisualTestContext};

use super::input_e2e::{Runtime, open_view};

#[gpui::test]
fn settings_lists_local_runtimes_and_protects_this_window(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    view.update_in(cx, |view, window, cx| view.open_settings(true, window, cx));
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds("settings-processes")
        .expect("Settings must offer Processes");
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    wait(cx, "processes-refresh");
    wait(cx, "process-used");
    let processes = seer::processes::list().unwrap();
    let active = processes
        .iter()
        .find(|process| process.used_by(std::process::id()))
        .unwrap();
    assert!(
        cx.debug_bounds(Box::leak(
            format!("process-row-{}", active.pid()).into_boxed_str()
        ))
        .is_some()
    );
    assert!(
        cx.debug_bounds(Box::leak(
            format!("process-stop-{}", active.pid()).into_boxed_str()
        ))
        .is_none()
    );
    cx.simulate_keystrokes("tab enter");
    wait(cx, "process-used");
}

fn wait(cx: &mut VisualTestContext, selector: &'static str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if cx.debug_bounds(selector).is_some() {
            return;
        }
        assert!(Instant::now() < deadline, "missing {selector}");
        thread::sleep(Duration::from_millis(10));
    }
}

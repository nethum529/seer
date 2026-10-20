use std::{
    thread,
    time::{Duration, Instant},
};

use gpui::{TestAppContext, VisualTestContext};

use super::input_e2e::{Runtime, open_view};

#[gpui::test]
fn settings_protects_this_window_when_its_runtime_does_not_answer(cx: &mut TestAppContext) {
    let runtime = Runtime::start();
    let (view, cx) = open_view(&runtime, cx);
    let active = runtime.pid();
    let _paused = Paused::new(active);
    view.update_in(cx, |view, window, cx| view.open_settings(true, window, cx));
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds("settings-processes")
        .expect("Settings must offer Processes");
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    wait(cx, "processes-refresh");
    wait(cx, "processes-idle");
    assert!(
        cx.debug_bounds(Box::leak(format!("process-row-{active}").into_boxed_str()))
            .is_some()
    );
    assert!(
        cx.debug_bounds("process-used").is_some(),
        "a runtime without status must still show Used by this window"
    );
    assert!(
        cx.debug_bounds(Box::leak(format!("process-stop-{active}").into_boxed_str()))
            .is_none()
    );
    cx.simulate_keystrokes("tab enter");
    wait(cx, "process-used");
}

struct Paused(u32);

impl Paused {
    fn new(pid: u32) -> Self {
        assert!(
            std::process::Command::new("/bin/kill")
                .args(["-STOP", &pid.to_string()])
                .status()
                .unwrap()
                .success()
        );
        Self(pid)
    }
}

impl Drop for Paused {
    fn drop(&mut self) {
        let _ = std::process::Command::new("/bin/kill")
            .args(["-CONT", &self.0.to_string()])
            .status();
    }
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

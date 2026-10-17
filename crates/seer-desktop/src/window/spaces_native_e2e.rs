use super::{SeerWindow, sidebar_native_e2e::post_shortcut};
use gpui::WindowHandle;
use seer_core::InputEvent;
use std::{fs, path::Path, time::Duration};

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    root: &Path,
    evidence: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let input = root.join("spaces-input");
    let ready = root.join("spaces-ready");
    let done = root.join("spaces-done");
    let script = root.join("spaces.py");
    fs::write(&script, format!(
        "import os,tty,termios\nfrom pathlib import Path\nold=termios.tcgetattr(0)\ntry:\n tty.setraw(0)\n f=open({:?},'ab',buffering=0)\n Path({:?}).touch()\n while True:\n  b=os.read(0,1)\n  if b==bytes([4]): break\n  f.write(b)\nfinally:\n termios.tcsetattr(0,termios.TCSANOW,old)\n Path({:?}).touch()\n",
        input.to_str().unwrap(), ready.to_str().unwrap(), done.to_str().unwrap()
    )).unwrap();
    handle
        .update(cx, |view, window, cx| {
            assert!(!view.settings_open());
            window.focus(&view.input.focus);
            view.send_input(
                InputEvent::Text(format!("python3 {}\r", script.display())),
                cx,
            );
        })
        .unwrap();
    for _ in 0..100 {
        if ready.exists() {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    assert!(ready.exists(), "raw PTY recorder must be ready");
    handle
        .update(cx, |_, _, _| {
            for (key, flags) in [
                (0x00, 0),
                (0x31, 0),
                (0x0b, 0),
                (0x31, 1 << 17),
                (0x08, 0),
                (0x31, 1 << 18),
                (0x02, 0),
                (0x31, 1 << 19),
                (0x0e, 0),
            ] {
                post_shortcut(key, flags);
            }
        })
        .unwrap();
    for _ in 0..100 {
        if fs::read(&input).unwrap_or_default().ends_with(b"e") {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    let bytes = fs::read(&input).unwrap();
    fs::write(evidence.join("native-spaces.bin"), &bytes).unwrap();
    assert_eq!(
        bytes, b"a b c\0d\xc2\xa0e",
        "native text spaces must reach the PTY"
    );
    handle
        .update(cx, |view, _, cx| {
            view.send_input(InputEvent::Text("\x04".into()), cx)
        })
        .unwrap();
    for _ in 0..100 {
        if done.exists() {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    assert!(done.exists(), "raw PTY recorder must stop");
}

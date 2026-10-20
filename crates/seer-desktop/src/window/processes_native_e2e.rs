use super::SeerWindow;
use gpui::WindowHandle;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

struct Process(u32, Option<thread::JoinHandle<()>>);
impl Process {
    fn spawn(command: &mut Command) -> Self {
        let mut child = command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        Self(
            pid,
            Some(thread::spawn(move || {
                let _ = child.wait();
            })),
        )
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(reaper) = self.1.take() {
            if !reaper.is_finished() {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", &self.0.to_string()])
                    .output();
            }
            reaper.join().unwrap();
        }
    }
}

pub(super) async fn exercise(
    handle: WindowHandle<SeerWindow>,
    root: &Path,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let source = root.join("window.c");
    fs::write(
        &source,
        "#include <unistd.h>\nint main(void) { for (;;) pause(); }\n",
    )
    .unwrap();
    let binary = root.join("seer");
    assert!(
        Command::new("cc")
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .status()
            .unwrap()
            .success()
    );
    let orphan = Process::spawn(&mut Command::new(&binary));
    let broker_binary = root.join("seer-broker");
    fs::copy(&binary, &broker_binary).unwrap();
    let broker_config = root.join("broker.toml");
    fs::write(
        &broker_config,
        format!("state_dir = '{}'\n", root.join("broker-state").display()),
    )
    .unwrap();
    let broker = Process::spawn(Command::new(&broker_binary).arg(broker_config));

    let socket = root.join("other/socket");
    fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let runtime_binary = std::env::var_os("SEER_RUNTIME_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("seer-runtime")
        });
    let runtime = Process::spawn(
        Command::new(runtime_binary)
            .arg(&socket)
            .args(["other", "/bin/sh", "qa"])
            .env_remove("SEER_ROOM_ENDPOINT")
            .env_remove("SEER_SNAPSHOT_DIR"),
    );
    let attach = r#"import socket,struct,json,sys,time
s=socket.socket(socket.AF_UNIX)
for _ in range(100):
 try: s.connect(sys.argv[1]); break
 except OSError: time.sleep(.02)
def read():
 n=struct.unpack('>I',s.recv(4))[0]; b=b''
 while len(b)<n: b+=s.recv(n-len(b))
 return json.loads(b)
read(); b=b'"AttachRuntime"'; s.sendall(struct.pack('>I',len(b))+b)
while 'Tree' not in read(): pass
"#;
    assert!(
        Command::new("python3")
            .args(["-c", attach])
            .arg(&socket)
            .status()
            .unwrap()
            .success()
    );
    fs::write(root.join("slow-ps"), []).unwrap();
    handle
        .update(cx, |view, window, cx| view.open_settings(true, window, cx))
        .unwrap();
    click(handle, "settings-processes", cx).await;
    key(handle, "tab", cx).await;
    handle
        .update(cx, |view, window, _| {
            assert!(
                view.appearance.processes.busy,
                "process discovery must still be running"
            );
            assert!(
                view.appearance.processes.refresh.is_focused(window),
                "Tab must work while process discovery waits"
            );
        })
        .unwrap();
    super::appearance_native_e2e::pause(handle, cx).await;
    super::appearance_native_e2e::snapshot(capture, evidence, "processes-loading");
    fs::remove_file(root.join("slow-ps")).unwrap();
    wait(handle, "process-used", cx).await;
    wait(handle, &format!("process-stop-{}", runtime.0), cx).await;
    snapshot(handle, evidence, capture, "processes-rows", cx).await;
    for width in [321., 479.] {
        handle
            .update(cx, |_, window, _| {
                window.resize(gpui::size(gpui::px(width), gpui::px(640.)))
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(250))
            .await;
        super::appearance_native_e2e::snapshot(
            capture,
            evidence,
            &format!("processes-narrow-{width}"),
        );
    }
    handle
        .update(cx, |_, window, _| {
            window.resize(gpui::size(gpui::px(960.), gpui::px(640.)))
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;

    check_stop(handle, root, runtime.0, evidence, capture, cx).await;
    snapshot(handle, evidence, capture, "processes-clean", cx).await;
    check_clean(handle, root, orphan.0, broker.0, evidence, capture, cx).await;
    drop(broker);
    drop(orphan);
    drop(runtime);
    snapshot(handle, evidence, capture, "processes-after", cx).await;
    key(handle, "escape", cx).await;
}

async fn check_stop(
    handle: WindowHandle<SeerWindow>,
    root: &Path,
    pid: u32,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    let socket = root.join("other/socket");
    let children = Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid="])
        .output()
        .unwrap();
    let shell: String = String::from_utf8(children.stdout)
        .unwrap()
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let child = fields.next()?;
            (fields.next()?.parse::<u32>().ok()? == pid).then(|| child.to_owned())
        })
        .expect("the other runtime must own a shell");
    click(handle, &format!("process-stop-{}", pid), cx).await;
    handle
        .update(cx, |view, window, _| {
            assert!(
                view.appearance
                    .processes
                    .sheet
                    .as_ref()
                    .unwrap()
                    .cancel
                    .is_focused(window)
            );
        })
        .unwrap();
    snapshot(handle, evidence, capture, "processes-stop", cx).await;
    key(handle, "enter", cx).await;
    wait(handle, "processes-sheet-closed", cx).await;
    assert!(socket.exists(), "Cancel must preserve the runtime");
    click(handle, &format!("process-stop-{}", pid), cx).await;
    fs::write(root.join("fail-ps"), []).unwrap();
    key(handle, "tab", cx).await;
    key(handle, "enter", cx).await;
    wait(handle, "processes-error", cx).await;
    handle
        .update(cx, |view, _, _| {
            assert!(view.appearance.processes.error.is_some());
            assert!(view.appearance.processes.sheet.is_some());
        })
        .unwrap();
    assert!(socket.exists(), "a failed stop must keep the runtime");
    super::appearance_native_e2e::pause(handle, cx).await;
    super::appearance_native_e2e::snapshot(capture, evidence, "processes-stop-error");
    fs::remove_file(root.join("fail-ps")).unwrap();
    key(handle, "enter", cx).await;
    wait(handle, "processes-sheet-closed", cx).await;
    assert!(!socket.exists(), "Stop must end the chosen runtime");
    assert!(
        !Command::new("/bin/kill")
            .args(["-0", &shell])
            .output()
            .unwrap()
            .status
            .success(),
        "Stop must end that runtime's shell"
    );
    wait(handle, "process-used", cx).await;
}

async fn check_clean(
    handle: WindowHandle<SeerWindow>,
    root: &Path,
    orphan: u32,
    broker: u32,
    evidence: &Path,
    capture: &Path,
    cx: &mut gpui::AsyncApp,
) {
    fs::write(root.join("fail-ps"), []).unwrap();
    click(handle, "processes-clean", cx).await;
    wait(handle, "processes-error", cx).await;
    handle
        .update(cx, |view, _, _| {
            assert!(view.appearance.processes.error.is_some())
        })
        .unwrap();
    super::appearance_native_e2e::pause(handle, cx).await;
    super::appearance_native_e2e::snapshot(capture, evidence, "processes-clean-error");
    fs::remove_file(root.join("fail-ps")).unwrap();
    click(handle, "processes-clean", cx).await;
    wait(handle, "processes-idle", cx).await;
    handle
        .update(cx, |view, _, _| {
            assert!(
                !view
                    .appearance
                    .processes
                    .rows
                    .iter()
                    .any(|row| row.process.pid() == orphan)
            );
            assert!(!view.appearance.processes.can_clean());
        })
        .unwrap();
    assert!(
        Command::new("/bin/kill")
            .args(["-0", &broker.to_string()])
            .output()
            .unwrap()
            .status
            .success(),
        "Clean up must preserve the room server"
    );
}

async fn key(handle: WindowHandle<SeerWindow>, key: &str, cx: &mut gpui::AsyncApp) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        handle
            .update(cx, |_, window, cx| {
                window.activate_window();
                cx.activate(true);
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
        if handle
            .update(cx, |_, window, _| {
                if !window.is_window_active() {
                    return false;
                }
                super::sidebar_native_e2e::post_shortcut(
                    match key {
                        "tab" => 0x30,
                        "enter" => 0x24,
                        "escape" => 0x35,
                        _ => panic!("unknown key"),
                    },
                    0,
                );
                true
            })
            .unwrap()
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the fixture window must receive native keys"
        );
    }
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
}
async fn click(handle: WindowHandle<SeerWindow>, selector: &str, cx: &mut gpui::AsyncApp) {
    if let Some(pid) = selector.strip_prefix("process-stop-") {
        let pid: u32 = pid.parse().unwrap();
        handle
            .update(cx, |view, _, cx| {
                let index = view
                    .appearance
                    .processes
                    .rows
                    .iter()
                    .position(|row| row.process.pid() == pid)
                    .unwrap();
                view.appearance.scroll.scroll_to_item(index + 1);
                cx.notify();
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
    }
    handle
        .update(cx, |view, window, _| {
            let position = if selector == "settings-processes" {
                gpui::point(gpui::px(170.), gpui::px(193.))
            } else {
                let scroll = &view.appearance.scroll;
                let index = selector
                    .strip_prefix("process-stop-")
                    .map(|pid| {
                        view.appearance
                            .processes
                            .rows
                            .iter()
                            .position(|row| row.process.pid().to_string() == pid)
                            .unwrap()
                            + 1
                    })
                    .unwrap_or(0);
                let bounds = scroll.bounds_for_item(index).unwrap() + scroll.offset();
                if index > 0 {
                    gpui::point(bounds.right() - gpui::px(25.), bounds.top() + gpui::px(16.))
                } else {
                    gpui::point(
                        bounds.left()
                            + gpui::px(if selector == "processes-clean" {
                                128.
                            } else {
                                40.
                            }),
                        bounds.top() + gpui::px(16.),
                    )
                }
            };
            for kind in [1, 2] {
                super::appearance_native_e2e::post_mouse(window, position, kind);
            }
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
}
async fn wait(handle: WindowHandle<SeerWindow>, selector: &str, cx: &mut gpui::AsyncApp) {
    for _ in 0..200 {
        if handle
            .update(cx, |view, _, _| match selector {
                "processes-idle" => !view.appearance.processes.busy,
                "processes-sheet-closed" => view.appearance.processes.sheet.is_none(),
                "processes-error" => view.appearance.processes.error.is_some(),
                "process-used" => view
                    .appearance
                    .processes
                    .rows
                    .iter()
                    .any(|row| row.process.used_by(std::process::id())),
                _ => view.appearance.processes.rows.iter().any(|row| {
                    format!("process-stop-{}", row.process.pid()) == selector && row.can_stop()
                }),
            })
            .unwrap()
        {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
    panic!("missing {selector}");
}
async fn snapshot(
    handle: WindowHandle<SeerWindow>,
    evidence: &Path,
    capture: &Path,
    name: &str,
    cx: &mut gpui::AsyncApp,
) {
    super::appearance_native_e2e::pause(handle, cx).await;
    super::appearance_native_e2e::snapshot(capture, evidence, name);
    handle
        .update(cx, |view, window, _| {
            super::processes_pixel_samples::record(view, window, evidence, name);
        })
        .unwrap();
}

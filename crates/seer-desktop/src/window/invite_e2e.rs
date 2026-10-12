use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use gpui::TestAppContext;
use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};

use super::{SeerWindow, input_e2e::Runtime, invite::OpenInvite};

#[gpui::test]
fn invite_stays_in_the_attached_room_after_a_cli_join(cx: &mut TestAppContext) {
    if std::env::var_os("SEER_INVITE_E2E_CHILD").is_none() {
        let directory = tempfile::Builder::new()
            .prefix("s454")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::create_dir(directory.path().join("home")).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "window::invite_e2e::invite_stays_in_the_attached_room_after_a_cli_join",
                "--nocapture",
            ])
            .env("SEER_INVITE_E2E_CHILD", "1")
            .env("HOME", directory.path().join("home"))
            .env("XDG_CONFIG_HOME", directory.path().join("config"))
            .env("XDG_STATE_HOME", directory.path().join("state"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let room_a = room("e2e", "owner-a");
    let room_b = room("guest", "guest-b");
    cli_join(&room_a);
    let runtime = Runtime::start();
    let _ = runtime.connect();
    let local =
        PathBuf::from(std::env::var_os("XDG_STATE_HOME").unwrap()).join("seer/runtimes/e2e");
    std::fs::create_dir_all(&local).unwrap();
    std::os::unix::fs::symlink(
        runtime.directory.path().join("socket"),
        local.join("socket"),
    )
    .unwrap();
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::assets::load_fonts(cx).unwrap();
        SeerWindow::new(window, cx)
    });
    cli_join(&room_b);
    cx.simulate_keystrokes("tab");
    cx.dispatch_action(OpenInvite);
    cx.simulate_keystrokes("tab enter");
    let deadline = Instant::now() + Duration::from_secs(5);
    while cx.debug_bounds("copy-join").is_none() {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "invite result did not appear");
        thread::sleep(Duration::from_millis(10));
    }
    cx.simulate_keystrokes("enter");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("seer join {room_a}")),
        "the existing window must invite to its attached room"
    );
}

fn cli_join(capsule: &str) {
    let binary = std::env::var_os("SEER_CLI_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/seer")
        });
    let mut child = Command::new(binary)
        .args(["join", capsule])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"Host\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn room(user: &str, credential: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let capsule = format!(
        "SEER1-127.0.0.1-{}-seat",
        listener.local_addr().unwrap().port()
    );
    let issued = capsule.clone();
    let user = user.to_owned();
    let credential = credential.to_owned();
    thread::spawn(move || {
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let message: ClientMsg = codec::decode(&mut stream).unwrap();
            let reply = match message {
                ClientMsg::Hello { user_id, .. } if user_id.is_empty() => ServerMsg::Refused {
                    reason: "credentials required".into(),
                },
                ClientMsg::Join { name, .. } => ServerMsg::Joined {
                    user_id: user.clone(),
                    credential: credential.clone(),
                    name,
                },
                ClientMsg::Hello {
                    user_id,
                    credential: found,
                    ..
                } => {
                    assert_eq!(user_id, user);
                    assert_eq!(found, credential);
                    ServerMsg::Welcome {
                        user_id: user.clone(),
                        name: "Host".into(),
                        client_id: "client".into(),
                        tree: Tree::new(),
                    }
                }
                _ => panic!("unexpected room request"),
            };
            codec::encode(&mut stream, &reply).unwrap();
            if let Ok(ClientMsg::Invite { hours: Some(24) }) = codec::decode(&mut stream) {
                codec::encode(
                    &mut stream,
                    &ServerMsg::Seat {
                        capsule: issued.clone(),
                        expires_in_secs: 86400,
                    },
                )
                .unwrap();
            }
        }
    });
    capsule
}

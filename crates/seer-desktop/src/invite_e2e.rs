use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

use gpui::{Context, TestAppContext};
use seer::ServerEntry;
use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};

use crate::invite::InviteSheet;

#[gpui::test]
fn invite_sheet_validates_custom_hours(cx: &mut TestAppContext) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let broker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let _: ClientMsg = codec::decode(&mut stream).unwrap();
        codec::encode(
            &mut stream,
            &ServerMsg::Welcome {
                user_id: "host".into(),
                name: "Host".into(),
                client_id: "client".into(),
                tree: Tree::new(),
            },
        )
        .unwrap();
        assert!(matches!(
            codec::decode::<_, ClientMsg>(&mut stream).unwrap(),
            ClientMsg::Invite { hours: Some(168) }
        ));
        codec::encode(
            &mut stream,
            &ServerMsg::Seat {
                capsule: "SEER2-test".into(),
                expires_in_secs: 604800,
            },
        )
        .unwrap();
    });
    let server: ServerEntry = serde_json::from_value(serde_json::json!({
        "endpoint": address, "alias": "Room", "user_id": "host", "name": "Host", "credential": "secret", "current": true
    })).unwrap();
    let (_, cx) = cx.add_window_view(|window, cx: &mut Context<InviteSheet>| {
        crate::assets::load_fonts(cx).unwrap();
        InviteSheet::new(server, true, window, cx)
    });
    cx.simulate_keystrokes("right right tab secondary-a 0 enter");
    assert!(cx.debug_bounds("invite-error").is_some());
    cx.simulate_keystrokes("secondary-a 1 6 9 enter");
    assert!(cx.debug_bounds("invite-error").is_some());
    cx.simulate_keystrokes("secondary-a 1 6 8 enter");
    let deadline = Instant::now() + Duration::from_secs(5);
    while cx.debug_bounds("copy-join").is_none() {
        cx.run_until_parked();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    broker.join().unwrap();
}

#[gpui::test]
fn invite_sheet_retries_and_copies_both_full_lines(cx: &mut TestAppContext) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let capsule = format!("SEER2-{}-{}", "a".repeat(64), "b".repeat(64));
    let issued = capsule.clone();
    let broker = thread::spawn(move || {
        for refused in [true, false] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            assert!(matches!(
                codec::decode::<_, ClientMsg>(&mut stream).unwrap(),
                ClientMsg::Hello { .. }
            ));
            codec::encode(
                &mut stream,
                &ServerMsg::Welcome {
                    user_id: "host".into(),
                    name: "Host".into(),
                    client_id: "client".into(),
                    tree: Tree::new(),
                },
            )
            .unwrap();
            assert!(matches!(
                codec::decode::<_, ClientMsg>(&mut stream).unwrap(),
                ClientMsg::Invite { hours: Some(24) }
            ));
            let reply = if refused {
                ServerMsg::Refused {
                    reason: "owner access required".into(),
                }
            } else {
                ServerMsg::Seat {
                    capsule: issued.clone(),
                    expires_in_secs: 86400,
                }
            };
            codec::encode(&mut stream, &reply).unwrap();
        }
    });
    let server: ServerEntry = serde_json::from_value(serde_json::json!({
        "endpoint": address, "alias": "Room", "user_id": "host", "name": "Host", "credential": "secret", "current": true
    })).unwrap();
    let (_, cx) = cx.add_window_view(|window, cx: &mut Context<InviteSheet>| {
        crate::assets::load_fonts(cx).unwrap();
        InviteSheet::new(server, true, window, cx)
    });
    cx.simulate_keystrokes("tab enter");
    let deadline = Instant::now() + Duration::from_secs(5);
    while cx.debug_bounds("invite-error").is_none() {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "invite error did not appear");
        thread::sleep(Duration::from_millis(10));
    }
    cx.simulate_keystrokes("enter");
    while cx.debug_bounds("copy-join").is_none() {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "invite result did not appear");
        thread::sleep(Duration::from_millis(10));
    }
    cx.simulate_keystrokes("enter");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("seer join {capsule}"))
    );
    cx.simulate_keystrokes("tab space");
    let install = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap();
    assert_eq!(
        install,
        format!(
            "curl -fsSL https://raw.githubusercontent.com/nethum529/seer-releases/main/install.sh | sh -s -- {capsule}"
        )
    );
    broker.join().unwrap();
}

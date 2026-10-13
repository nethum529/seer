use std::net::TcpListener;
use std::process::Command;
use std::thread;
use std::time::Duration;

use seer::ServerEntry;
use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[test]
fn desktop_and_cli_make_the_same_invitation() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let capsule = format!(
        "SEER1-127.0.0.1-{}-seat",
        listener.local_addr().unwrap().port()
    );
    let issued = capsule.clone();
    let broker = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            assert!(
                matches!(codec::decode::<_, ClientMsg>(&mut stream).unwrap(), ClientMsg::Hello { user_id, credential, .. } if user_id == "host" && credential == "secret")
            );
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
            codec::encode(
                &mut stream,
                &ServerMsg::Seat {
                    capsule: issued.clone(),
                    expires_in_secs: 86400,
                },
            )
            .unwrap();
        }
    });
    let server: ServerEntry = serde_json::from_value(serde_json::json!({
        "endpoint": address, "alias": "Room", "user_id": "host", "name": "Host", "credential": "secret", "current": true
    })).unwrap();
    let invite = seer::create_invite(&server, Some(24)).unwrap();
    assert_eq!(invite.join_line(), format!("seer join {capsule}"));
    assert!(
        invite
            .install_line()
            .ends_with(&format!("| sh -s -- {capsule}"))
    );
    assert_eq!(invite.expires_in_secs, 86400);

    let directory = std::env::temp_dir().join(format!("s454-{}", std::process::id()));
    std::fs::create_dir_all(directory.join("seer")).unwrap();
    std::fs::write(directory.join("seer/servers.toml"), format!(
        "[[servers]]\nendpoint = {address:?}\nalias = \"Room\"\nuser_id = \"host\"\nname = \"Host\"\ncredential = \"secret\"\ncurrent = true\n"
    )).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_seer"))
        .args(["invite", "--hours", "24"])
        .env("XDG_CONFIG_HOME", &directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8(output.stdout).unwrap();
    assert!(printed.lines().any(|line| line == invite.join_line()));
    assert!(printed.lines().any(|line| line == invite.install_line()));
    broker.join().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, Person, PersonState, ServerMsg, codec};

#[test]
fn peek_opens_the_desktop_with_the_room_and_person_selected() {
    let root = std::env::temp_dir().join(format!("seer450-peek-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let cli = root.join("seer-cli");
    fs::copy(env!("CARGO_BIN_EXE_seer"), &cli).unwrap();
    let args = root.join("opened");
    let desktop = root.join("seer-desktop");
    fs::write(
        &desktop,
        format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n", args.display()),
    )
    .unwrap();
    fs::set_permissions(&desktop, fs::Permissions::from_mode(0o700)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    fs::create_dir(root.join("seer")).unwrap();
    fs::write(root.join("seer/servers.toml"), format!(
        "[[servers]]\nendpoint = '{}'\nalias = 'Room'\nuser_id = 'own'\nname = 'You'\ncredential = 'secret'\ncurrent = true\n", listener.local_addr().unwrap())).unwrap();
    let broker = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let hello: ClientMsg = codec::decode(&mut socket).unwrap();
        assert!(matches!(hello, ClientMsg::Hello { .. }));
        codec::encode(
            &mut socket,
            &ServerMsg::Welcome {
                user_id: "own".into(),
                name: "You".into(),
                client_id: "cli".into(),
                tree: Tree::new(),
            },
        )
        .unwrap();
        let request: ClientMsg = codec::decode(&mut socket).unwrap();
        assert_eq!(request, ClientMsg::ListPeople);
        codec::encode(
            &mut socket,
            &ServerMsg::People {
                people: vec![Person {
                    user_id: "remote".into(),
                    name: "Kyler".into(),
                    host: true,
                    online: true,
                    idle_secs: 0,
                    attached_clients: 1,
                    peekable: true,
                    state: PersonState::Active,
                    tabs: 1,
                    foreground: "shell".into(),
                }],
            },
        )
        .unwrap();
    });
    let result = Command::new(cli)
        .args(["peek", "Kyler"])
        .env("XDG_CONFIG_HOME", &root)
        .env("XDG_STATE_HOME", root.join("state"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    broker.join().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !args.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fs::read_to_string(args).unwrap(),
        "--user\nown\n--peek\nKyler\n"
    );
    fs::remove_dir_all(root).unwrap();
}

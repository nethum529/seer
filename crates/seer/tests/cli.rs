use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::thread;

use seer_core::proto::{ClientMsg, ServerMsg};

#[path = "support/cli.rs"]
mod cli_support;
#[path = "support/server_io.rs"]
mod server_io;

use cli_support::{
    TestConfig, accept, assert_hello, listener, person, run, send, send_welcome, text,
};
use server_io::receive;

#[test]
fn help_detach_and_missing_attach_have_exact_results() {
    let config = TestConfig::new();

    let help = run(&config, &["--help"], "");
    assert_eq!(help.status.code(), Some(0));
    let help_text = text(&help.stdout);
    for command in [
        "start", "invite", "join", "list", "attach", "detach", "peek",
    ] {
        assert!(help_text.contains(command));
    }

    let detach = run(&config, &["detach"], "");
    assert_eq!(detach.status.code(), Some(1));
    assert_eq!(detach.stderr, b"run seer join first\n");

    let attach = run(&config, &["attach"], "");
    assert_eq!(attach.status.code(), Some(1));
    assert_eq!(attach.stderr, b"run seer join first\n");

    let invalid = run(&config, &["unknown"], "");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    assert!(text(&invalid.stderr).starts_with("Usage: seer <command>\n"));
    for command in ["start", "invite", "list", "attach", "detach"] {
        let extra = run(&config, &[command, "extra"], "");
        assert_eq!(extra.status.code(), Some(2));
        assert!(text(&extra.stderr).contains("join [capsule] Join a server"));
    }
}

#[test]
fn join_persists_private_store_without_seat_token_and_reconnects_after_restart() {
    let config = TestConfig::new();
    let listener =
        seer_net::Listener::bind(seer_net::SecretKey::generate()).expect("iroh listener must bind");
    let endpoint = listener.id().to_string();
    let alias = endpoint[..8].to_owned();
    let server = thread::spawn(move || {
        let (device_id, mut first) = listener.accept().expect("first client must connect");
        assert_eq!(
            receive(&mut first),
            ClientMsg::Join {
                seat_token: "seat-token".into(),
                name: "alice".into(),
            }
        );
        send(
            &mut first,
            &ServerMsg::Refused {
                reason: "name is in use".into(),
            },
        );

        let (retry_id, mut second) = listener.accept().expect("retry client must connect");
        assert_eq!(retry_id, device_id);
        assert_eq!(
            receive(&mut second),
            ClientMsg::Join {
                seat_token: "seat-token".into(),
                name: "bob".into(),
            }
        );
        send(
            &mut second,
            &ServerMsg::Joined {
                user_id: "user-bob".into(),
                credential: "device-secret".into(),
                name: "bob".into(),
            },
        );
        drop(second);

        let (attached_id, mut attached) = listener.accept().expect("first attach must connect");
        assert_eq!(attached_id, device_id);
        assert_eq!(
            receive(&mut attached),
            ClientMsg::Hello {
                user_id: "user-bob".into(),
                credential: "device-secret".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            }
        );
        send_welcome(&mut attached, "user-bob", "bob");
        let (restarted_id, mut restarted) = listener.accept().expect("second attach must connect");
        assert_eq!(restarted_id, device_id);
        assert_hello(&mut restarted);
        send_welcome(&mut restarted, "user-bob", "bob");
    });
    let capsule = format!("SEER2-{endpoint}-seat-token\nalice\nbob\n");
    let output = run(&config, &["join"], &capsule);

    assert!(output.status.success() && output.stderr.is_empty());
    assert_eq!(
        text(&output.stdout),
        format!(
            "Invitation: Server: iroh:{endpoint}\nName: That name is in use.\nName: Joined as bob. Attaching...\n"
        )
    );
    let store_path = config.root.join("seer/servers.toml");
    let store = fs::read_to_string(&store_path).expect("store must be readable");
    assert!(store.contains("credential = \"device-secret\""));
    assert!(!store.contains("seat-token"));
    let key_path = config.root.join("seer/device.key");
    assert_eq!(
        fs::read(&key_path)
            .expect("device key must be readable")
            .len(),
        32
    );
    assert_eq!(mode(&key_path), 0o600);
    let restarted = run(&config, &["attach"], "");
    assert_eq!(
        text(&restarted.stdout),
        format!("Attached to {alias} as bob.\n")
    );
    server.join().expect("server must finish");
    assert_eq!(mode(&store_path), 0o600);
    assert_eq!(mode(&config.root.join("seer")), 0o700);
}

#[test]
fn attach_uses_the_saved_identity() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    write_store(&config, &[saved(address.port(), "team.example.com", true)]);
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let mut stream = accept(&listener);
            assert_hello(&mut stream);
            send_welcome(&mut stream, "user-bob", "bob");
        }
    });

    let output = run(&config, &["attach"], "");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"Attached to team.example.com as bob.\n");
    assert!(output.stderr.is_empty());

    let first = SavedServer {
        endpoint: "127.0.0.1:1".into(),
        alias: "first".into(),
        current: false,
    };
    write_store(&config, &[first, saved(address.port(), "second", true)]);
    let current = run(&config, &["attach"], "");
    assert_eq!(current.status.code(), Some(0));
    assert_eq!(current.stdout, b"Attached to second as bob.\n");
    let first = SavedServer {
        endpoint: "127.0.0.1:1".into(),
        alias: "first".into(),
        current: false,
    };
    write_store(&config, &[first, saved(address.port(), "second", false)]);

    let output = run(&config, &["attach"], "2\n");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        b"Select a server:\n  1. first (bob)\n  2. second (bob)\nServer: Attached to second as bob.\n"
    );
    assert!(output.stderr.is_empty());
    server.join().expect("server must finish");
}

#[test]
fn list_prints_people_and_marks_an_unreachable_server() {
    let config = TestConfig::new();
    let first_listener = listener();
    let address = first_listener
        .local_addr()
        .expect("listener must have an address");
    let second_listener = listener();
    let second_address = second_listener
        .local_addr()
        .expect("second listener must have an address");
    let offline = SavedServer {
        endpoint: "127.0.0.1:1".into(),
        alias: "offline".into(),
        current: false,
    };
    write_store(
        &config,
        &[
            offline,
            saved(address.port(), "team.example.com", true),
            saved(second_address.port(), "second", false),
        ],
    );
    let server = thread::spawn(move || {
        let mut stream = accept(&first_listener);
        assert_hello(&mut stream);
        send_welcome(&mut stream, "user-bob", "bob");
        assert_eq!(receive(&mut stream), ClientMsg::ListPeople);
        send(
            &mut stream,
            &ServerMsg::People {
                people: vec![
                    person("user-bob", "bob", 0),
                    person("user-alice", "alice", 2),
                ],
            },
        );
    });
    let second_server = thread::spawn(move || {
        let mut stream = accept(&second_listener);
        assert_hello(&mut stream);
        send_welcome(&mut stream, "user-bob", "bob");
        assert_eq!(receive(&mut stream), ClientMsg::ListPeople);
        send(
            &mut stream,
            &ServerMsg::People {
                people: vec![person("user-bob", "bob", 1)],
            },
        );
    });

    let output = run(&config, &["list"], "");

    assert_eq!(output.status.code(), Some(0));
    let stdout = text(&output.stdout);
    assert!(stdout.contains("SERVER            YOU  STATE        PEOPLE\n"));
    assert!(stdout.contains("team.example.com  bob  detached     alice, bob\n"));
    assert!(stdout.contains("offline           bob  unreachable"));
    assert!(stdout.contains("second            bob  detached"));
    assert!(
        stdout
            .find("team.example.com")
            .expect("current row must exist")
            < stdout.find("offline").expect("offline row must exist")
    );
    assert!(!stdout.contains("device-secret"));
    assert!(output.stderr.is_empty());
    server.join().expect("server must finish");
    second_server.join().expect("second server must finish");
}

#[test]
fn invite_prints_the_worked_example_block() {
    let config = TestConfig::new();
    let listener = listener();
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    write_store(&config, &[saved(address.port(), "team.example.com", true)]);
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        assert_hello(&mut stream);
        send_welcome(&mut stream, "user-bob", "bob");
        assert_eq!(receive(&mut stream), ClientMsg::Invite { hours: None });
        send(
            &mut stream,
            &ServerMsg::Seat {
                capsule: "SEER1-team.example.com-7321-A7K4Q9P2".into(),
                expires_in_secs: 3_600,
            },
        );
    });

    let output = run(&config, &["invite"], "");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        b"Seat ready. It works once and expires in 1 hour.\nSend this to a friend:\n\nPaste this in Terminal:\ncurl -fsSL https://raw.githubusercontent.com/nethum529/seer-releases/main/install.sh | sh -s -- SEER1-team.example.com-7321-A7K4Q9P2\n"
    );
    assert!(output.stderr.is_empty());
    server.join().expect("server must finish");
}

fn write_store(config: &TestConfig, servers: &[SavedServer]) {
    let directory = config.root.join("seer");
    fs::create_dir_all(&directory).expect("config directory must exist");
    let mut contents = String::new();
    for server in servers {
        contents.push_str(&format!(
            "[[servers]]\nendpoint = \"{}\"\nalias = \"{}\"\nuser_id = \"user-bob\"\nname = \"bob\"\ncredential = \"device-secret\"\ncurrent = {}\n",
            server.endpoint, server.alias, server.current
        ));
    }
    fs::write(directory.join("servers.toml"), contents).expect("store must be written");
}

fn saved(port: u16, alias: &str, current: bool) -> SavedServer {
    SavedServer {
        endpoint: format!("127.0.0.1:{port}"),
        alias: alias.into(),
        current,
    }
}

struct SavedServer {
    endpoint: String,
    alias: String,
    current: bool,
}

fn mode(path: &Path) -> u32 {
    let metadata = fs::metadata(path).expect("metadata must load");
    metadata.permissions().mode() & 0o777
}

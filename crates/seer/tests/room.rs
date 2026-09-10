#![cfg(target_os = "linux")]

use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

struct Host {
    root: PathBuf,
    port: u16,
}

impl Host {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(format!("/tmp/s346-{name}-{}", std::process::id()));
        let port = TcpListener::bind("127.0.0.1:0")
            .expect("port probe must bind")
            .local_addr()
            .expect("port probe must have an address")
            .port();
        fs::create_dir_all(root.join("c/seer")).expect("config directory must be created");
        fs::create_dir_all(root.join("s/seer")).expect("state directory must be created");
        fs::copy(env!("CARGO_BIN_EXE_seer"), root.join("seer"))
            .expect("client binary must be copied");
        let launcher = root.join("seer-broker");
        fs::write(
            &launcher,
            format!(
                "#!/bin/sh\nexec '{}' \"$@\"\n",
                env!("CARGO_BIN_EXE_seer-broker")
            ),
        )
        .expect("broker launcher must be written");
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700))
            .expect("broker launcher must be executable");
        fs::write(
            root.join("c/seer/broker.toml"),
            format!(
                "listen = \"127.0.0.1:{port}\"\npublished_addr = \"127.0.0.1:{port}\"\nremote = false\nowner_name = \"alice\"\nstate_dir = {:?}\n",
                root.join("s/seer")
            ),
        )
        .expect("broker config must be written");
        Self { root, port }
    }

    fn run_as(&self, home: &str, user: &str, args: &[&str]) -> Output {
        let home = self.root.join(home);
        let mut child = Command::new(self.root.join("seer"))
            .args(args)
            .env("XDG_CONFIG_HOME", home.join("c"))
            .env("XDG_STATE_HOME", home.join("s"))
            .env("HOME", &home)
            .env("USER", user)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("seer must start");
        child
            .stdin
            .take()
            .expect("stdin must be piped")
            .write_all(b"\n")
            .expect("default name must be sent");
        child.wait_with_output().expect("seer must finish")
    }

    fn run(&self, args: &[&str]) -> String {
        let output = self.run_as("", "alice", args);
        assert!(
            output.status.success(),
            "seer {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("output must be UTF-8")
    }

    fn invite(&self) -> String {
        self.run(&["invite"])
            .split_whitespace()
            .find(|word| word.starts_with("SEER1-"))
            .expect("invite must print a capsule")
            .to_owned()
    }

    fn join_as_bob(&self, capsule: &str) -> Output {
        self.run_as("bob", "bob", &["join", capsule])
    }

    fn people_on_disk(&self) -> Vec<String> {
        let registry: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(self.root.join("s/seer/registry.json"))
                .expect("registry must be readable"),
        )
        .expect("registry must parse");
        registry["people"]
            .as_array()
            .expect("registry people")
            .iter()
            .map(|person| person["name"].as_str().expect("person name").to_owned())
            .collect()
    }

    fn seed_old_people(&self) {
        fs::write(
            self.root.join("s/seer/registry.json"),
            "{\"people\":[{\"user_id\":\"old-id\",\"name\":\"alice\",\"credential_hash\":\"aa\",\"created_at\":1,\"is_owner\":true},{\"user_id\":\"don-id\",\"name\":\"don\",\"credential_hash\":\"bb\",\"created_at\":2,\"is_owner\":false}],\"seats\":[]}\n",
        )
        .expect("old registry must be written");
        fs::write(
            self.root.join("c/seer/servers.toml"),
            format!(
                "[[servers]]\nendpoint = \"127.0.0.1:{}\"\nalias = \"host\"\nuser_id = \"old-id\"\nname = \"alice\"\ncredential = \"old-secret\"\ncurrent = true\n",
                self.port
            ),
        )
        .expect("old owner store must be written");
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.run_as("", "alice", &["stop"]);
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_plain_start_drops_the_old_people() {
    let host = Host::new("drop");
    host.seed_old_people();

    host.run(&["start"]);

    assert_eq!(host.people_on_disk(), ["alice"]);
    assert!(host.run(&["list"]).contains("detached  alice"));
}

#[test]
fn a_plain_start_leaves_a_working_owner_credential() {
    let host = Host::new("owner");
    host.run(&["start"]);
    host.run(&["stop"]);

    host.run(&["start"]);

    assert!(host.run(&["invite"]).contains("Seat ready."));
}

#[test]
fn a_seat_made_before_a_stop_still_works_after_a_plain_start() {
    let host = Host::new("seat");
    host.run(&["start"]);
    let capsule = host.invite();
    host.run(&["stop"]);

    host.run(&["start"]);

    let joined = host.join_as_bob(&capsule);
    assert!(
        String::from_utf8_lossy(&joined.stdout).contains("Joined as bob."),
        "{}",
        String::from_utf8_lossy(&joined.stderr)
    );
    assert_eq!(host.people_on_disk(), ["alice", "bob"]);
}

#[test]
fn restore_keeps_the_members() {
    let host = Host::new("restore");
    host.run(&["start"]);
    let capsule = host.invite();
    assert!(host.join_as_bob(&capsule).status.success());
    host.run(&["stop"]);

    host.run(&["start", "--restore"]);

    assert_eq!(host.people_on_disk(), ["alice", "bob"]);
    assert!(host.run(&["list"]).contains("bob"));
}

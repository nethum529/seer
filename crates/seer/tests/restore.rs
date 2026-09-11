#![cfg(target_os = "linux")]

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

struct Host {
    root: PathBuf,
}

impl Host {
    fn command(&self) -> Command {
        let mut command = Command::new(self.root.join("seer"));
        command
            .env("XDG_CONFIG_HOME", self.root.join("c"))
            .env("XDG_STATE_HOME", self.root.join("s"))
            .env("HOME", &self.root)
            .env("USER", "alice")
            .env("SEER_TEST_BROKER", env!("CARGO_BIN_EXE_seer-broker"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn run(&self, args: &[&str]) {
        let mut child = self.command().args(args).spawn().expect("seer must start");
        child
            .stdin
            .take()
            .expect("stdin must be piped")
            .write_all(b"\n")
            .expect("default name must be sent");
        let output = child.wait_with_output().expect("seer must finish");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.command().arg("stop").output();
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn restore_recovers_an_owner_credential_that_authenticates() {
    let host = Host {
        root: PathBuf::from(format!("/tmp/s318-{}", std::process::id())),
    };
    fs::create_dir(&host.root).expect("test directory must be created");
    fs::copy(env!("CARGO_BIN_EXE_seer"), host.root.join("seer"))
        .expect("client binary must be copied");
    let launcher = host.root.join("seer-broker");
    fs::write(
        &launcher,
        "#!/bin/sh\nsed -i 's/remote = true/remote = false/' \"$1\"\nexec \"$SEER_TEST_BROKER\" \"$@\"\n",
    )
    .expect("local broker launcher must be written");
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700))
        .expect("broker launcher must be executable");
    host.run(&["start"]);
    let path = host.root.join("c/seer/servers.toml");
    let original: toml::Value =
        toml::from_str(&fs::read_to_string(&path).expect("owner store must exist"))
            .expect("owner store must parse");
    let owner_id = original["servers"][0]["user_id"]
        .as_str()
        .expect("owner ID");
    host.run(&["stop"]);
    fs::remove_file(&path).expect("owner store must be deleted");
    host.run(&["start", "--restore"]);

    let restored: toml::Value =
        toml::from_str(&fs::read_to_string(path).expect("owner store must be restored"))
            .expect("restored store must parse");
    let entries = restored["servers"].as_array().expect("server entries");
    assert_eq!(entries.len(), 1);
    let owner = &entries[0];
    assert_eq!(owner["user_id"].as_str(), Some(owner_id));
    let mut stream = TcpStream::connect(owner["endpoint"].as_str().expect("endpoint"))
        .expect("broker must accept connections");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout must be set");
    codec::encode(
        &mut stream,
        &ClientMsg::Hello {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            user_id: owner_id.to_owned(),
            credential: owner["credential"].as_str().expect("credential").to_owned(),
        },
    )
    .expect("hello must be sent");
    let reply: ServerMsg = codec::decode(&mut stream).expect("broker must reply");
    assert!(matches!(reply, ServerMsg::Welcome { user_id, .. } if user_id == owner_id));
}

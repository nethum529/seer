use seer_core::proto::{ClientMsg, ServerMsg, codec};
use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
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
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn run(&self, args: &[&str]) -> String {
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
        String::from_utf8_lossy(&output.stdout).into_owned()
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
    let host = start_host();
    let path = host.root.join("c/seer/servers.toml");
    let original: toml::Value =
        toml::from_str(&fs::read_to_string(&path).expect("owner store must exist"))
            .expect("owner store must parse");
    let owner_id = original["servers"][0]["user_id"]
        .as_str()
        .expect("owner ID");
    fs::remove_file(&path).expect("owner store must be deleted");
    host.run(&["stop"]);
    let output = host.run(&["start", "--restore"]);
    assert!(output.starts_with("Server started.\n"), "{output}");

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
    host_stop_ends_runtime(&host, owner_id);
}

fn host_stop_ends_runtime(host: &Host, owner_id: &str) {
    let directory = host.root.join("s/seer/runtimes").join(owner_id);
    fs::create_dir_all(&directory).expect("runtime directory must exist");
    let socket = directory.join("socket");
    let child = Command::new(env!("CARGO_BIN_EXE_seer-runtime"))
        .arg(&socket)
        .arg(owner_id)
        .args(["/bin/sh", "stop-test"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let mut runtime = Runtime(child);
    fs::write(directory.join("runtime.pid"), runtime.0.id().to_string())
        .expect("runtime PID must be saved");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&socket) {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            let ready: ServerMsg = codec::decode(&mut stream).expect("runtime must answer");
            assert!(matches!(ready, ServerMsg::RuntimeReady { .. }));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "runtime must become ready"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    host.run(&["stop"]);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while runtime
        .0
        .try_wait()
        .expect("runtime status must be available")
        .is_none()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "host stop must end the host runtime"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

struct Runtime(std::process::Child);

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_host() -> Host {
    let host = Host {
        root: PathBuf::from(format!(
            "/tmp/s318-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )),
    };
    fs::create_dir(&host.root).expect("test directory must be created");
    fs::copy(env!("CARGO_BIN_EXE_seer"), host.root.join("seer"))
        .expect("client binary must be copied");
    fs::copy(
        env!("CARGO_BIN_EXE_seer-broker"),
        host.root.join("seer-broker"),
    )
    .expect("broker binary must be copied");
    let address = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let config_dir = host.root.join("c/seer");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("broker.toml"), format!(
        "listen = \"{address}\"\npublished_addr = \"{address}\"\nremote = false\nstate_dir = \"{}\"\nowner_name = \"alice\"\n", host.root.join("s/seer").display()
    )).unwrap();
    host.run(&["start"]);
    host
}

#[test]
fn removed_person_stays_removed_after_start_restore() {
    let host = start_host();
    let saved: toml::Value =
        toml::from_str(&fs::read_to_string(host.root.join("c/seer/servers.toml")).unwrap())
            .unwrap();
    let endpoint = saved["servers"][0]["endpoint"].as_str().unwrap();
    let invitation = host.run(&["invite"]);
    let capsule = invitation
        .lines()
        .find_map(|line| line.strip_prefix("seer join "))
        .unwrap();
    let token = capsule.rsplit('-').next().unwrap();
    let mut guest = TcpStream::connect(endpoint).unwrap();
    guest
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    codec::encode(
        &mut guest,
        &ClientMsg::Join {
            seat_token: token.into(),
            name: "Guest".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        },
    )
    .unwrap();
    let ServerMsg::Joined {
        user_id,
        credential,
        ..
    } = codec::decode(&mut guest).unwrap()
    else {
        panic!("Guest must join");
    };
    host.run(&["remove", "Guest"]);
    host.run(&["stop"]);
    host.run(&["start", "--restore"]);
    let mut guest = TcpStream::connect(endpoint).unwrap();
    guest
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    codec::encode(
        &mut guest,
        &ClientMsg::Hello {
            user_id,
            credential,
            version: env!("CARGO_PKG_VERSION").into(),
        },
    )
    .unwrap();
    assert!(matches!(codec::decode::<_, ServerMsg>(&mut guest).unwrap(),
        ServerMsg::Refused { reason } if reason.contains("host removed you")));
    assert!(!host.run(&["list"]).contains("Guest"));
}

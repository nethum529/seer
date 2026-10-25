use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[path = "support/cli_run.rs"]
mod cli_run;
#[path = "support/process.rs"]
mod process;

use cli_run::{TestConfig, run, text};
use process::{process_exists, wait_for_process_end};

fn run_with_grace(config: &TestConfig, args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_seer"))
        .args(args)
        .env("HOME", &config.root)
        .env("XDG_CONFIG_HOME", &config.root)
        .env("XDG_STATE_HOME", config.root.join("state-home"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output.stderr));
}

struct Room(TestConfig);

impl Room {
    fn new() -> Self {
        let config = TestConfig::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let state = config.root.join("state-home/seer");
        fs::create_dir_all(config.root.join("seer")).unwrap();
        fs::write(
            config.root.join("seer/broker.toml"),
            format!(
                "listen = \"{address}\"\npublished_addr = \"{address}\"\nremote = false\nowner_name = \"alice\"\nstate_dir = {state:?}\n"
            ),
        )
        .unwrap();
        drop(listener);
        let room = Self(config);
        room.run(&["start"]);
        room
    }

    fn run(&self, args: &[&str]) -> String {
        let output = run(&self.0, args, "\n");
        assert!(output.status.success(), "{}", text(&output.stderr));
        text(&output.stdout)
    }

    fn server(&self) -> toml::Value {
        let store: toml::Value =
            toml::from_str(&fs::read_to_string(self.0.root.join("seer/servers.toml")).unwrap())
                .unwrap();
        store["servers"][0].clone()
    }

    fn broker_pid(&self) -> i32 {
        fs::read_to_string(self.0.root.join("state-home/seer/broker.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
}

struct Runtime {
    pid: i32,
    directory: PathBuf,
    reaper: Option<thread::JoinHandle<()>>,
    jobs: Vec<i32>,
}

impl Runtime {
    fn new(config: &TestConfig, user: &str, stubborn_shell: bool) -> Self {
        let directory = config.root.join("state-home/seer/runtimes").join(user);
        fs::create_dir_all(&directory).unwrap();
        let shell = if stubborn_shell {
            let shell = config.root.join("shell");
            fs::write(
                &shell,
                "#!/bin/sh\ntrap '' HUP TERM\nsleep 600 &\necho \"$$ $!\" > \"$HOME/jobs\"\nwait\n",
            )
            .unwrap();
            fs::set_permissions(&shell, fs::Permissions::from_mode(0o700)).unwrap();
            shell
        } else {
            PathBuf::from("/bin/sh")
        };
        let mut child = Command::new(env!("CARGO_BIN_EXE_seer-runtime"))
            .arg(directory.join("socket"))
            .arg(user)
            .arg(shell)
            .arg("cleanup")
            .env("HOME", &config.root)
            .env("XDG_CONFIG_HOME", &config.root)
            .env("XDG_STATE_HOME", config.root.join("state-home"))
            .env("SEER_SNAPSHOT_DIR", &directory)
            .env_remove("SEER_ROOM_ENDPOINT")
            .env_remove("SEER_ROOM_CREDENTIAL")
            .env_remove("SEER_ROOM_KEY")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = i32::try_from(child.id()).unwrap();
        fs::write(directory.join("runtime.pid"), pid.to_string()).unwrap();
        let runtime = Self {
            pid,
            directory,
            reaper: Some(thread::spawn(move || {
                child.wait().unwrap();
            })),
            jobs: Vec::new(),
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(mut socket) = UnixStream::connect(runtime.directory.join("socket")) {
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let ready: ServerMsg = codec::decode(&mut socket).unwrap();
                assert!(matches!(ready, ServerMsg::RuntimeReady { .. }));
                codec::encode(&mut socket, &ClientMsg::AttachRuntime).unwrap();
                let tree: ServerMsg = codec::decode(&mut socket).unwrap();
                assert!(matches!(tree, ServerMsg::Tree { .. }));
                return runtime;
            }
            assert!(Instant::now() < deadline, "runtime must start");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn read_jobs(&mut self, config: &TestConfig) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(jobs) = fs::read_to_string(config.root.join("jobs")) {
                self.jobs = jobs
                    .split_whitespace()
                    .map(|pid| pid.parse().unwrap())
                    .collect();
                if self.jobs.len() == 2 {
                    return;
                }
            }
            assert!(Instant::now() < deadline, "shell must start its job");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        for pid in self.jobs.iter().chain(std::iter::once(&self.pid)) {
            // SAFETY: these PIDs belong to processes started by this test.
            unsafe { libc::kill(*pid, libc::SIGKILL) };
        }
        if let Some(reaper) = self.reaper.take() {
            reaper.join().unwrap();
        }
    }
}

#[test]
fn stop_removes_runtime_markers_and_keeps_the_layout_for_restore() {
    let room = Room::new();
    let runtime = Runtime::new(&room.0, room.server()["user_id"].as_str().unwrap(), false);

    room.run(&["stop"]);

    wait_for_process_end(runtime.pid);
    assert!(!runtime.directory.join("socket").exists());
    assert!(!runtime.directory.join("runtime.pid").exists());
    assert!(runtime.directory.join("session.json").exists());
    room.run(&["start", "--restore"]);
    assert_eq!(
        room.server()["user_id"].as_str().unwrap(),
        runtime.directory.file_name().unwrap()
    );
}

#[test]
fn stop_ends_shell_jobs_even_when_the_runtime_socket_is_missing() {
    let room = Room::new();
    let mut runtime = Runtime::new(&room.0, room.server()["user_id"].as_str().unwrap(), true);
    runtime.read_jobs(&room.0);
    fs::remove_file(runtime.directory.join("socket")).unwrap();

    run_with_grace(&room.0, &["stop"]);

    wait_for_process_end(runtime.pid);
    for job in &runtime.jobs {
        wait_for_process_end(*job);
    }
}

#[test]
fn ps_stop_removes_runtime_markers() {
    let config = TestConfig::new();
    let runtime = Runtime::new(&config, "ps-owner", false);
    let stopped = run(&config, &["ps", "--stop", &runtime.pid.to_string()], "");
    assert!(stopped.status.success(), "{}", text(&stopped.stderr));
    assert!(!runtime.directory.join("runtime.pid").exists());
    assert!(runtime.directory.join("session.json").exists());
}

#[test]
fn ps_stop_ends_jobs_that_ignore_terminal_hangup() {
    let config = TestConfig::new();
    let mut runtime = Runtime::new(&config, "jobs-owner", true);
    runtime.read_jobs(&config);

    run_with_grace(&config, &["ps", "--stop", &runtime.pid.to_string()]);
    for job in &runtime.jobs {
        wait_for_process_end(*job);
    }
}

#[test]
fn stop_finds_its_runtime_when_the_pid_file_names_another_runtime() {
    let room = Room::new();
    let runtime = Runtime::new(&room.0, room.server()["user_id"].as_str().unwrap(), false);
    let other = Runtime::new(&room.0, "another-room", false);
    fs::write(runtime.directory.join("runtime.pid"), other.pid.to_string()).unwrap();

    room.run(&["stop"]);

    wait_for_process_end(runtime.pid);
    assert!(process_exists(other.pid));
}

#[test]
#[cfg(target_os = "linux")]
fn a_failed_runtime_start_does_not_leave_a_background_process() {
    let room = Room::new();
    let user = room.server()["user_id"].as_str().unwrap().to_owned();
    let directory = room.0.root.join("state-home/seer/runtimes").join(user);
    fs::create_dir_all(directory.join("runtime.pid")).unwrap();

    let output = Command::new("script")
        .args([
            "-qec",
            &format!("{} attach", env!("CARGO_BIN_EXE_seer")),
            "/dev/null",
        ])
        .env("HOME", &room.0.root)
        .env("XDG_CONFIG_HOME", &room.0.root)
        .env("XDG_STATE_HOME", room.0.root.join("state-home"))
        .env("TERM", "xterm")
        .stdin(Stdio::null())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(UnixStream::connect(directory.join("socket")).is_err());
}

#[test]
#[cfg(target_os = "linux")]
fn a_failed_second_start_preserves_the_existing_runtime() {
    let room = Room::new();
    let runtime = Runtime::new(&room.0, room.server()["user_id"].as_str().unwrap(), false);
    // SAFETY: the PID belongs to the runtime started by this test.
    unsafe { libc::kill(runtime.pid, libc::SIGSTOP) };

    let output = Command::new("script")
        .args([
            "-qec",
            &format!("{} attach", env!("CARGO_BIN_EXE_seer")),
            "/dev/null",
        ])
        .env("HOME", &room.0.root)
        .env("XDG_CONFIG_HOME", &room.0.root)
        .env("XDG_STATE_HOME", room.0.root.join("state-home"))
        .env("TERM", "xterm")
        .stdin(Stdio::null())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(process_exists(runtime.pid));
    assert!(runtime.directory.join("socket").exists());
    assert!(runtime.directory.join("runtime.pid").exists());
}

#[test]
fn leaving_a_room_removes_its_local_runtime_state() {
    let room = Room::new();
    let guest = TestConfig::new();
    let invitation = room.run(&["invite"]);
    let capsule = invitation
        .split_whitespace()
        .find(|word| word.starts_with("SEER1-"))
        .unwrap();
    let joined = run(&guest, &["join", capsule], "bob\n");
    assert!(joined.status.success(), "{}", text(&joined.stderr));
    let store: toml::Value =
        toml::from_str(&fs::read_to_string(guest.root.join("seer/servers.toml")).unwrap()).unwrap();
    let runtime = Runtime::new(
        &guest,
        store["servers"][0]["user_id"].as_str().unwrap(),
        false,
    );

    let left = run(&guest, &["leave"], "");

    assert!(left.status.success(), "{}", text(&left.stderr));
    wait_for_process_end(runtime.pid);
    assert!(!runtime.directory.exists());
    assert!(process_exists(room.broker_pid()));
}

#[test]
fn starting_a_new_room_cleans_the_replaced_local_identity() {
    let room = Room::new();
    let runtime = Runtime::new(&room.0, room.server()["user_id"].as_str().unwrap(), false);
    // SAFETY: the PID belongs to the broker started by this test.
    unsafe { libc::kill(room.broker_pid(), libc::SIGKILL) };
    wait_for_process_end(room.broker_pid());

    room.run(&["start"]);

    wait_for_process_end(runtime.pid);
    assert!(!runtime.directory.exists());
}

#[test]
fn stop_cleans_stale_files_after_the_broker_and_runtime_crash() {
    let room = Room::new();
    let runtime = Runtime::new(&room.0, room.server()["user_id"].as_str().unwrap(), false);
    for pid in [runtime.pid, room.broker_pid()] {
        // SAFETY: these PIDs belong to processes started by this test.
        unsafe { libc::kill(pid, libc::SIGKILL) };
        wait_for_process_end(pid);
    }

    let stopped = run(&room.0, &["stop"], "");

    assert!(!stopped.status.success());
    assert!(!runtime.directory.join("socket").exists());
    assert!(!runtime.directory.join("runtime.pid").exists());
    assert!(!room.0.root.join("state-home/seer/broker.pid").exists());
    assert!(runtime.directory.join("session.json").exists());
}

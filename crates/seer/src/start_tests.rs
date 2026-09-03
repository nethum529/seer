use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{BrokerConfig, ServersFile, listening_inode, running_broker, save_owner};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[test]
fn running_check_rejects_a_pid_that_does_not_own_the_port() {
    let directory = test_directory();
    let listener = TcpListener::bind("127.0.0.1:0").expect("test listener must bind");
    let listen = listener
        .local_addr()
        .expect("test listener must have an address");
    fs::write(directory.join("broker.pid"), "999999\n").expect("test pid must be written");
    let config = BrokerConfig {
        listen,
        published_addr: listen.to_string(),
        remote: false,
        owner_name: "alice".to_owned(),
        state_dir: directory.clone(),
    };

    assert!(!running_broker(&config));

    fs::remove_dir_all(directory).expect("test directory must be removed");
}

#[test]
fn proc_line_must_have_the_port_and_listen_state() {
    let listening = "0: 0100007F:1C99 00000000:0000 0A 0 0 0 0 0 12345";
    let connected = "0: 0100007F:1C99 00000000:0000 01 0 0 0 0 0 12345";

    assert_eq!(listening_inode(listening, 7321).as_deref(), Some("12345"));
    assert_eq!(listening_inode(listening, 7322), None);
    assert_eq!(listening_inode(connected, 7321), None);
}

#[test]
fn owner_save_creates_a_missing_store() {
    let directory = test_directory();
    let config = BrokerConfig {
        listen: "127.0.0.1:7321".parse().expect("test address must parse"),
        published_addr: "host.test:7321".to_owned(),
        remote: false,
        owner_name: "alice".to_owned(),
        state_dir: directory.join("state"),
    };

    save_owner(&directory, &config, "secret".to_owned()).expect("owner must be saved");

    let contents =
        fs::read_to_string(directory.join("servers.toml")).expect("server store must be written");
    let store: ServersFile = toml::from_str(&contents).expect("server store must parse");
    assert_eq!(store.servers.len(), 1);
    fs::remove_dir_all(directory).expect("test directory must be removed");
}

fn test_directory() -> PathBuf {
    let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let path = PathBuf::from(format!("/tmp/s73u-{}-{number}", std::process::id()));
    fs::create_dir(&path).expect("test directory must be created");
    path
}

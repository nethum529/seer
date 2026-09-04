use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{BrokerConfig, save_owner};
use crate::store::ServerStore;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

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

    let store =
        ServerStore::load_from(&directory.join("servers.toml")).expect("server store must parse");
    assert_eq!(store.servers.len(), 1);
    fs::remove_dir_all(directory).expect("test directory must be removed");
}

fn test_directory() -> PathBuf {
    let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let path = PathBuf::from(format!("/tmp/s73u-{}-{number}", std::process::id()));
    fs::create_dir(&path).expect("test directory must be created");
    path
}

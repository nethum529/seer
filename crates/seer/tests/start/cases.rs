use super::*;

#[test]
fn prompt_defaults_create_config_and_owner_store() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    ensure_default_port_is_free();
    let executable = install_binaries(&directory);
    write_existing_servers(&directory);

    let output = run_start(&executable, &directory, "\n", &[]);

    assert!(output.status.success(), "{}", output.stderr);
    assert!(output.stdout.contains("Your name [alice]: "));
    assert!(!output.stdout.contains("Published address"));
    assert!(
        output
            .stdout
            .starts_with("Your name [alice]: Server started.\nYou are alice.\n"),
        "{}",
        output.stdout
    );
    assert!(
        output
            .stdout
            .lines()
            .any(|line| line.starts_with("Ready in ") && line.ends_with(" s.")),
        "{}",
        output.stdout
    );
    let broker: toml::Value = read_toml(directory.config_home().join("seer/broker.toml"));
    assert_eq!(broker["listen"].as_str(), Some("127.0.0.1:7321"));
    assert_eq!(broker["published_addr"].as_str(), Some("127.0.0.1:7321"));
    assert_eq!(broker["remote"].as_bool(), Some(true));
    assert_eq!(broker["owner_name"].as_str(), Some("alice"));
    assert_eq!(broker["state_dir"].as_str(), directory.state_dir().to_str());
    assert_saved_servers(&directory);
    assert_eq!(mode(directory.config_home().join("seer")), 0o700);
    assert_eq!(
        mode(directory.config_home().join("seer/servers.toml")),
        0o600
    );
    let log = fs::read_to_string(directory.state_dir().join("broker.log"))
        .expect("broker log must be read");
    assert!(log.contains("broker-output"));
    assert!(!log.contains("owner-credential"));
}

#[test]
fn a_new_room_drops_the_people_of_the_old_room_and_keeps_the_seats() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    ensure_default_port_is_free();
    fs::create_dir_all(directory.state_dir()).expect("state directory must be created");
    let registry_path = directory.state_dir().join("registry.json");
    fs::write(
        &registry_path,
        "{\"people\":[{\"user_id\":\"old-id\",\"name\":\"don\",\"credential_hash\":\"aa\",\"created_at\":1,\"is_owner\":true}],\"seats\":[{\"token_hash\":\"cc\",\"expires_at\":9999999999,\"used\":false}]}",
    )
    .expect("old registry must be written");
    let executable = install_binaries(&directory);

    let output = run_start(&executable, &directory, "\n", &[]);

    assert!(output.status.success(), "{}", output.stderr);
    let registry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(registry_path).expect("registry must be read"))
            .expect("registry must parse");
    assert_eq!(registry["people"].as_array().map(Vec::len), Some(0));
    assert_eq!(registry["seats"].as_array().map(Vec::len), Some(1));
}

#[test]
fn second_start_uses_the_live_broker() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    let address = unused_address();
    write_config(&directory, address);
    fs::write(directory.pid_path(), "999999\n").expect("stale pid must be written");
    let executable = install_binaries(&directory);
    let first = run_start(&executable, &directory, "", &[]);
    assert!(first.status.success(), "{}", first.stderr);
    let first_pid = fs::read_to_string(directory.pid_path()).expect("pid must exist");
    assert_ne!(first_pid, "999999\n");

    let second = run_start(&executable, &directory, "", &[]);

    assert!(second.status.success(), "{}", second.stderr);
    assert_eq!(second.stdout, "Server already running.\n");
    assert_eq!(
        fs::read_to_string(directory.pid_path()).expect("pid must still exist"),
        first_pid
    );
    assert_eq!(launch_count(&directory), 1);
}

#[test]
fn failed_broker_prints_only_the_last_twenty_log_lines() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    let address = unused_address();
    write_config(&directory, address);
    let executable = install_binaries(&directory);
    let output = run_start(&executable, &directory, "", &[("SEER_FAKE_FAIL", "1")]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, "");
    assert!(!output.stderr.contains("failure-line-1\n"));
    assert!(output.stderr.contains("failure-line-6\n"));
    assert!(output.stderr.contains("failure-line-25\n"));
}

#[test]
fn remote_listener_failure_does_not_report_start_success() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    write_config(&directory, unused_address());
    let executable = install_binaries(&directory);
    let environment = [("SEER_FAKE_REMOTE_FAILURE", "1")];
    let output = run_start(&executable, &directory, "", &environment);
    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stdout.contains("Server started"));
}

#[test]
fn invalid_config_is_not_replaced() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    let config_dir = directory.config_home().join("seer");
    fs::create_dir_all(&config_dir).expect("config directory must be created");
    let config_path = config_dir.join("broker.toml");
    fs::write(&config_path, [0xff]).expect("invalid config must be written");
    let executable = install_binaries(&directory);

    let output = run_start(&executable, &directory, "\n\n", &[]);

    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stdout.contains("Your name"));
    assert_eq!(fs::read(config_path).expect("config must remain"), [0xff]);
}

#[test]
fn unreadable_server_store_is_not_replaced() {
    let _serial = PROCESS_TEST.lock().expect("process test lock must work");
    let directory = TestDirectory::new();
    ensure_default_port_is_free();
    let config_dir = directory.config_home().join("seer");
    fs::create_dir_all(&config_dir).expect("config directory must be created");
    let store_path = config_dir.join("servers.toml");
    fs::write(&store_path, [0xff]).expect("invalid store must be written");
    let executable = install_binaries(&directory);

    let output = run_start(
        &executable,
        &directory,
        "\n\n",
        &[("SEER_FAKE_DESCENDANT", "1")],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read(store_path).expect("store must remain"), [0xff]);
    for path in ["fake.pid", "worker.pid"] {
        let pid = fs::read_to_string(directory.state_dir().join(path))
            .expect("fake process pid must exist")
            .trim()
            .parse::<i32>()
            .expect("fake process pid must parse");
        assert!(!process_exists(pid), "broker process group must stop");
    }
}

fn assert_saved_servers(directory: &TestDirectory) {
    let servers: toml::Value = read_toml(directory.config_home().join("seer/servers.toml"));
    let owner = &servers["servers"][0];
    assert_eq!(owner["endpoint"].as_str(), Some("other.test:8000"));
    assert_eq!(owner["current"].as_bool(), Some(false));
    let local = &servers["servers"][1];
    assert_eq!(local["endpoint"].as_str(), Some("127.0.0.1:7321"));
    assert_eq!(local["alias"].as_str(), Some("host.test"));
    assert_eq!(local["user_id"].as_str(), Some("owner-id"));
    assert_eq!(local["name"].as_str(), Some("alice"));
    assert_eq!(local["credential"].as_str(), Some("owner-secret"));
    assert_eq!(local["current"].as_bool(), Some(true));
    assert_eq!(servers["servers"].as_array().map(Vec::len), Some(2));
}

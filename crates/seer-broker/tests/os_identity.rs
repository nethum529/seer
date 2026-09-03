#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::ServerMsg;

#[path = "support/binary.rs"]
mod binary;
#[path = "forwarding/support.rs"]
mod support;

use support::{
    ProcessGuard, TestFiles, command_output, connect_when_ready, current_os_user, read_message,
    send_hello, unused_address, wait_for_disconnect, wait_for_tree_with_tab, welcome_client_id,
};

struct OsAccount {
    name: String,
    uid: String,
    gid: String,
    home: String,
    shell: String,
}

fn current_os_account() -> OsAccount {
    let name = current_os_user();
    let uid = command_output("id", &["-u"]);
    let gid = command_output("id", &["-g"]);
    let record = command_output("getent", &["passwd", &name]);
    let fields = record.split(':').collect::<Vec<_>>();
    assert_eq!(fields.len(), 7, "password record must have seven fields");
    OsAccount {
        name,
        uid,
        gid,
        home: fields[5].to_owned(),
        shell: fields[6].to_owned(),
    }
}

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[test]
fn uses_the_mapped_os_identity_and_rejects_an_unsafe_account() {
    let temporary = TestFiles::new();
    let account = current_os_account();
    let unsafe_account = "x".repeat(65);
    let address = unused_address();
    temporary.write_config_with_os_users(address, &account.name, &unsafe_account);
    temporary.write_runtime_wrapper();
    let broker = temporary.start_broker();
    let _broker = ProcessGuard::new(broker);

    let mut rejected = connect_when_ready(address);
    send_hello(&mut rejected, "bob", "bob-secret");
    drop(welcome_client_id(read_message(&mut rejected), "bob"));
    assert!(
        matches!(read_message(&mut rejected), ServerMsg::Refused { .. }),
        "an unsafe mapping must be refused"
    );
    wait_for_disconnect(&mut rejected);

    let mut accepted = connect_when_ready(address);
    send_hello(&mut accepted, "alice", "alice-secret");
    drop(welcome_client_id(read_message(&mut accepted), "alice"));
    drop(wait_for_tree_with_tab(&mut accepted));
    assert_runtime_identity(&temporary, "alice", &account);
    temporary.terminate_runtime("alice");
}

fn assert_runtime_identity(temporary: &TestFiles, user: &str, account: &OsAccount) {
    let identity_file = temporary.root.join(format!("{user}.identity"));
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while Instant::now() < deadline && !identity_file.is_file() {
        thread::sleep(POLL_INTERVAL);
    }
    let identity = fs::read_to_string(identity_file).expect("runtime identity must read");
    assert_eq!(
        identity.lines().collect::<Vec<_>>(),
        [
            account.uid.as_str(),
            account.home.as_str(),
            account.shell.as_str()
        ]
    );
    let state = temporary.state_dir.join("users").join(user);
    let metadata = fs::metadata(state).expect("runtime state metadata must load");
    assert_eq!(metadata.uid().to_string(), account.uid);
    assert_eq!(metadata.gid().to_string(), account.gid);
    assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
}

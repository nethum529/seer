use std::ffi::CStr;
use std::fs;
use std::process::Stdio;

use crate::runtime_socket_helpers::*;
use crate::support::*;

const GENERATION: &str = "0123456789abcdef0123456789abcdef";
const PROBE: &str = "SEER_LOGIN_PROBE=login; export SEER_LOGIN_PROBE\n";

// Issue 448: an app opened from the Dock or a desktop launcher starts the
// runtime with a small environment. Each new terminal must run the account's
// login shell as a login shell, so the profile files set PATH and tools.
#[test]
fn login_shell_value_starts_the_account_shell_as_a_login_shell() {
    let temporary = TemporaryDirectory::new();
    let home = temporary.path.join("home");
    write_login_files(&home);
    let socket_path = temporary.path.join("runtime.sock");
    let runtime = runtime_command()
        .env("HOME", &home)
        .env("SHELL", "/nonexistent/seer-shell")
        .env("PATH", "/usr/bin:/bin")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("ZDOTDIR")
        .env_remove("SEER_LOGIN_PROBE")
        .args([
            socket_path.as_os_str(),
            "alice".as_ref(),
            seer_runtime::LOGIN_SHELL.as_ref(),
            GENERATION.as_ref(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("runtime must start");
    let _runtime = RuntimeProcess::new(runtime);

    let mut stream = connect_with_timeout(&socket_path);
    let tree = read_until_tree(&mut stream);
    let workspace = &tree.workspaces[0];
    let tab = &workspace.tabs[0];
    send_input_at(
        &mut stream,
        &workspace.id,
        &tab.id,
        &tab.panes[0].id,
        "printf 'probe=[%s] shell=[%s]\\n' \"$SEER_LOGIN_PROBE\" \"$SHELL\"\n",
    );
    let expected = format!("probe=[login] shell=[{}]", account_shell());
    wait_for_cells_containing(&mut stream, &expected);
}

// Each file is read only by a login shell of its kind.
fn write_login_files(home: &std::path::Path) {
    let fish = home.join(".config/fish");
    fs::create_dir_all(&fish).expect("fish config directory must exist");
    for name in [".profile", ".bash_profile", ".zprofile"] {
        fs::write(home.join(name), PROBE).expect("login file must be written");
    }
    fs::write(
        fish.join("config.fish"),
        "status is-login; and set -gx SEER_LOGIN_PROBE login\n",
    )
    .expect("fish config must be written");
}

fn account_shell() -> String {
    // SAFETY: getpwuid returns a pointer into static storage or null, and the
    // test reads it before any other passwd call.
    let entry = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(
        !entry.is_null(),
        "the test account must have a passwd entry"
    );
    // SAFETY: a non-null passwd entry holds a valid C string in pw_shell.
    unsafe { CStr::from_ptr((*entry).pw_shell) }
        .to_string_lossy()
        .into_owned()
}

use std::fs;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_core::{InputEvent, TerminalInput};

const MARKERS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "CLAUDE_PROJECT_DIR",
    "CLAUDE_JOB_DIR",
    "AI_AGENT",
    "CODEX_CI",
    "CODEX_THREAD_ID",
    "CODEX_SESSION_ID",
    "CODEX_VERSION",
    "CODEX_PERMISSION_PROFILE",
    "CODEX_TOOL_CALL_ID",
    "CODEX_PLUGIN_METRICS_OUTPUT",
    "EXEC_WRAPPER",
    "CODEX_ESCALATE_SOCKET",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
];
const WAIT: Duration = Duration::from_secs(10);

#[test]
fn agent_markers_do_not_reach_shells_but_user_startup_values_stay() {
    for shell in ["/bin/bash", "/bin/zsh"] {
        if !std::path::Path::new(shell).is_file() {
            continue;
        }
        let (running, before, after) = shell_environment(shell, false);
        let activation = format!(
            "CLAUDE_ENV_FILE={}/activation.sh",
            running.directory.display()
        );
        for environment in [&before, &after] {
            assert!(
                environment.lines().any(|line| line == activation),
                "{shell} must keep the configured activation file: {environment}"
            );
        }
        for marker in MARKERS {
            assert!(
                !before
                    .lines()
                    .any(|line| line.starts_with(&format!("{marker}="))),
                "{shell} inherited {marker} before user startup: {before}"
            );
        }
        for entry in [
            "USER_STARTUP_VALUE=from-startup",
            "REVIEW_ACTIVATION=from-activation",
            "CLAUDE_CODE_CHILD_SESSION=user-child",
            "CODEX_THREAD_ID=user-thread",
            "EXEC_WRAPPER=/user/exec-wrapper",
            "CODEX_ESCALATE_SOCKET=42",
            "CLAUDE_CODE_EFFORT_LEVEL=high",
            "CLAUDE_CODE_TMPDIR=/tmp/user-claude",
            "CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS=1",
            "ANTHROPIC_API_KEY=user-key-sentinel",
            "SEER_USER_ID=alice",
        ] {
            assert!(
                after.lines().any(|line| line == entry),
                "{shell}: {entry}: {after}"
            );
        }
        assert!(after.contains(&format!("CODEX_HOME={}/codex", running.directory.display())));
        assert!(after.lines().any(|line| line.starts_with("SEER_PANE=")));
    }
}

#[test]
fn login_profile_wrapper_reaches_non_login_shells_without_codex_context() {
    for shell in ["/bin/bash", "/bin/zsh"] {
        if !std::path::Path::new(shell).is_file() {
            continue;
        }
        let (running, before, after) = shell_environment(shell, true);
        let launcher = fs::read_to_string(running.directory.join("launcher"))
            .expect("login profile must run before the runtime");
        let wrapper = format!("EXEC_WRAPPER={}/user-wrapper", running.directory.display());
        for environment in [&launcher, &before, &after] {
            assert!(
                environment.lines().any(|line| line == wrapper),
                "{shell} must keep the login profile wrapper: {environment}"
            );
            assert!(
                !environment
                    .lines()
                    .any(|line| line.starts_with("CODEX_ESCALATE_SOCKET=")),
                "{shell} must have no Codex socket: {environment}"
            );
        }
    }
}

fn shell_environment(shell: &str, inherited_profile: bool) -> (Running, String, String) {
    let directory = PathBuf::from(format!(
        "/tmp/seer-agent-env-{}-{}-{}",
        std::process::id(),
        shell.rsplit('/').next().expect("shell name"),
        if inherited_profile {
            "profile"
        } else {
            "codex"
        }
    ));
    fs::create_dir(&directory).expect("isolated home must exist");
    let mut running = Running {
        child: None,
        directory,
    };
    fs::write(
        running.directory.join("activation.sh"),
        "export REVIEW_ACTIVATION=from-activation\n",
    )
    .expect("activation file must exist");
    let startup = "env > \"$HOME/before\"\n\
        export USER_STARTUP_VALUE=from-startup\n\
        export CLAUDE_CODE_CHILD_SESSION=user-child CODEX_THREAD_ID=user-thread\n";
    let startup = if inherited_profile {
        startup.to_owned()
    } else {
        format!("{startup}export EXEC_WRAPPER=/user/exec-wrapper CODEX_ESCALATE_SOCKET=42\n")
    };
    for name in [".bashrc", ".zshenv"] {
        fs::write(running.directory.join(name), &startup).expect("startup file must exist");
    }
    fs::write(running.directory.join(".zshrc"), "HISTFILE=/dev/null\n")
        .expect("zsh history must stay disabled in the fixture");
    let socket = running.directory.join("socket");
    let mut command = if inherited_profile {
        for name in [".bash_profile", ".zprofile"] {
            fs::write(
                running.directory.join(name),
                "export EXEC_WRAPPER=\"$HOME/user-wrapper\"\nenv > \"$HOME/launcher\"\n",
            )
            .expect("login profile must exist");
        }
        let mut launcher = Command::new(shell);
        launcher.args(["-l", "-c", "exec \"$@\"", "seer-launch"]);
        launcher.arg(env!("CARGO_BIN_EXE_seer-runtime"));
        launcher
    } else {
        Command::new(env!("CARGO_BIN_EXE_seer-runtime"))
    };
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &running.directory)
        .env("XDG_CONFIG_HOME", running.directory.join("config"))
        .env("XDG_STATE_HOME", running.directory.join("state"))
        .env("SEER_SNAPSHOT_DIR", running.directory.join("snapshot"))
        .env("CLAUDE_CODE_EFFORT_LEVEL", "high")
        .env("CLAUDE_CODE_TMPDIR", "/tmp/user-claude")
        .env("CLAUDE_ENV_FILE", running.directory.join("activation.sh"))
        .env("CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS", "1")
        .env("CODEX_HOME", running.directory.join("codex"))
        .env("ANTHROPIC_API_KEY", "user-key-sentinel")
        .args([
            socket.as_os_str(),
            "alice".as_ref(),
            shell.as_ref(),
            "0123456789abcdef0123456789abcdef".as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for marker in MARKERS {
        if !inherited_profile || !["EXEC_WRAPPER", "CODEX_ESCALATE_SOCKET"].contains(marker) {
            command.env(marker, "outer-session");
        }
    }
    if !inherited_profile {
        command
            .env(
                "EXEC_WRAPPER",
                running.directory.join("codex-execve-wrapper"),
            )
            .env("CODEX_ESCALATE_SOCKET", "17");
    }
    running.child = Some(command.spawn().expect("runtime must start"));
    let deadline = Instant::now() + WAIT;
    let mut stream = loop {
        if let Ok(stream) = UnixStream::connect(&socket) {
            break stream;
        }
        assert!(Instant::now() < deadline, "runtime socket must open");
        std::thread::sleep(Duration::from_millis(10));
    };
    stream.set_read_timeout(Some(WAIT)).expect("read timeout");
    codec::encode(&mut stream, &ClientMsg::AttachRuntime).expect("attach runtime");
    let tree = loop {
        assert!(Instant::now() < deadline, "runtime must send its tree");
        if let ServerMsg::Tree { tree } = codec::decode(&mut stream).expect("runtime message") {
            break tree;
        }
    };
    let workspace = &tree.workspaces[0];
    let tab = &workspace.tabs[0];
    codec::encode(
        &mut stream,
        &ClientMsg::TerminalInput {
            workspace: workspace.id.clone(),
            tab: tab.id.clone(),
            pane: tab.panes[0].id.clone(),
            input: TerminalInput::new(InputEvent::Text(
                ". \"$CLAUDE_ENV_FILE\"; env > \"$HOME/after.tmp\"; mv \"$HOME/after.tmp\" \"$HOME/after\"\n".into(),
            )),
        },
    )
    .expect("shell command");
    let after = loop {
        if let Ok(text) = fs::read_to_string(running.directory.join("after")) {
            break text;
        }
        assert!(
            Instant::now() < deadline,
            "shell must write its environment"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let before = fs::read_to_string(running.directory.join("before")).expect("startup ran");
    (running, before, after)
}

struct Running {
    child: Option<Child>,
    directory: PathBuf,
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

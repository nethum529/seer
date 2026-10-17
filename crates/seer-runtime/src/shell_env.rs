use portable_pty::CommandBuilder;
use std::env;
use std::ffi::OsStr;
use std::path::Path;

// Issue 402: the runtime outlives the terminal that started it, so these
// markers of that terminal would reach every later Seer shell. Issue 512:
// agent session identity and control sockets have the same lifetime. Remove
// them before startup files run, so values set by those files stay.
const SESSION_PREFIXES: [&str; 7] = [
    "HERDR_",
    "TMUX",
    "ZELLIJ",
    "KITTY_",
    "WEZTERM_",
    "ITERM_",
    "ALACRITTY_",
];
const SESSION_NAMES: [&str; 27] = [
    "STY",
    "WINDOW",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
    "__CFBundleIdentifier",
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
    "CODEX_ESCALATE_SOCKET",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
];

pub(crate) fn remove_session_vars(command: &mut CommandBuilder) {
    // Codex exports this alias and its escalation socket together.
    if env::var_os("CODEX_ESCALATE_SOCKET").is_some()
        && env::var_os("EXEC_WRAPPER").is_some_and(|wrapper| {
            Path::new(&wrapper).file_name() == Some(OsStr::new("codex-execve-wrapper"))
        })
    {
        command.env_remove("EXEC_WRAPPER");
    }
    for (name, _) in env::vars_os() {
        let Some(name) = name.to_str() else {
            continue;
        };
        let is_session = SESSION_NAMES.contains(&name)
            || SESSION_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix));
        if is_session {
            command.env_remove(name);
        }
    }
}

use portable_pty::CommandBuilder;
use std::env;

// Issue 402: the runtime outlives the terminal that started it, so these
// markers of that terminal would reach every later Seer shell.
const SESSION_PREFIXES: [&str; 7] = [
    "HERDR_",
    "TMUX",
    "ZELLIJ",
    "KITTY_",
    "WEZTERM_",
    "ITERM_",
    "ALACRITTY_",
];
const SESSION_NAMES: [&str; 6] = [
    "STY",
    "WINDOW",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
    "__CFBundleIdentifier",
];

pub(crate) fn remove_session_vars(command: &mut CommandBuilder) {
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

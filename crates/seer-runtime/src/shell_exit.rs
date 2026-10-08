use portable_pty::CommandBuilder;
use std::env;
use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use crate::login_shell::Shell;

const BASH_RC_FILES: &str = "[ -f \"$HOME/.bashrc\" ] && . \"$HOME/.bashrc\"\n";
const BASH_LOGIN_FILES: &str = "[ -f /etc/profile ] && . /etc/profile\n\
    for seer_file in \"$HOME/.bash_profile\" \"$HOME/.bash_login\" \"$HOME/.profile\"; do\n\
    if [ -f \"$seer_file\" ]; then . \"$seer_file\"; break; fi\n\
    done\n\
    unset seer_file\n";

// Issue 403: a function named exit shadows the builtin in fish, bash and
// zsh. Only the interactive shell that Seer starts gets the function.
// Scripts and child shells keep the normal exit.
pub(crate) fn install(command: &mut CommandBuilder, shell: &Shell) -> io::Result<()> {
    let seer = seer_binary();
    let files = files_directory();
    let name = Path::new(&shell.program)
        .file_name()
        .and_then(|name| name.to_str());
    // bash reads no rcfile as a login shell, so Seer's rcfile reads the login
    // files itself.
    if shell.login && name != Some("bash") {
        command.arg("-l");
    }
    match name {
        Some("fish") => {
            let function = format!("function exit; {} exit; end", fish_quote(&seer));
            command.args(["-C", &function]);
        }
        Some("bash") => {
            let (rc, startup) = if shell.login {
                (files.join("bash_profile"), BASH_LOGIN_FILES)
            } else {
                (files.join("bashrc"), BASH_RC_FILES)
            };
            let body = format!("{startup}exit() {{ {} exit; }}\n", posix_quote(&seer));
            write_private(&files, &rc, &body)?;
            command.arg("--rcfile");
            command.arg(rc);
        }
        Some("zsh") => install_zsh(command, &files.join("zsh"), &seer)?,
        _ => {}
    }
    Ok(())
}

// zsh has no rcfile option. ZDOTDIR points at Seer's files, which load the
// user's own files and then restore ZDOTDIR for child shells.
fn install_zsh(command: &mut CommandBuilder, directory: &Path, seer: &Path) -> io::Result<()> {
    let user_home = "${SEER_ZDOTDIR:-$HOME}";
    let zshenv = format!("[ -f \"{user_home}/.zshenv\" ] && . \"{user_home}/.zshenv\"\n");
    let zprofile = format!("[ -f \"{user_home}/.zprofile\" ] && . \"{user_home}/.zprofile\"\n");
    let zshrc = format!(
        "if [ -n \"$SEER_ZDOTDIR\" ]; then ZDOTDIR=\"$SEER_ZDOTDIR\"; else unset ZDOTDIR; fi\n\
         unset SEER_ZDOTDIR\n\
         [ -f \"${{ZDOTDIR:-$HOME}}/.zshrc\" ] && . \"${{ZDOTDIR:-$HOME}}/.zshrc\"\n\
         exit() {{ {} exit; }}\n",
        posix_quote(seer)
    );
    write_private(directory, &directory.join(".zshenv"), &zshenv)?;
    write_private(directory, &directory.join(".zprofile"), &zprofile)?;
    write_private(directory, &directory.join(".zshrc"), &zshrc)?;
    if let Some(original) = env::var_os("ZDOTDIR") {
        command.env("SEER_ZDOTDIR", original);
    }
    command.env("ZDOTDIR", directory);
    Ok(())
}

fn files_directory() -> PathBuf {
    crate::snapshot_directory()
        .map(|directory| directory.join("shell"))
        .unwrap_or_else(|| env::temp_dir().join(format!("seer-shell-{}", std::process::id())))
}

fn write_private(directory: &Path, path: &Path, body: &str) -> io::Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)?;
    fs::write(path, body)
}

fn seer_binary() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|runtime| runtime.parent().map(|parent| parent.join("seer")))
        .filter(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("seer"))
}

fn posix_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn fish_quote(path: &Path) -> String {
    format!(
        "'{}'",
        path.to_string_lossy()
            .replace('\\', "\\\\")
            .replace('\'', "\\'")
    )
}

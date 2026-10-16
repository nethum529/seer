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
    // Apple's /bin/bash reads ENV only when it runs as sh, so it starts as a
    // plain login bash without the exit function.
    let env_bash = shell.login
        && name == Some("bash")
        && !(cfg!(target_os = "macos") && shell.program == "/bin/bash");
    // bash reads long options only before short options, so a login bash
    // gets --login with --posix.
    if shell.login && !env_bash {
        command.arg("-l");
    }
    match name {
        Some("fish") => {
            let function = format!("function exit; {} exit; end", fish_quote(&seer));
            command.args(["-C", &function]);
        }
        Some("bash") if env_bash => install_bash_login(command, &files, &seer)?,
        Some("bash") if shell.login => {}
        Some("bash") => {
            let rc = files.join("bashrc");
            let body = format!("{BASH_RC_FILES}exit() {{ {} exit; }}\n", posix_quote(&seer));
            write_private(&files, &rc, &body)?;
            command.arg("--rcfile");
            command.arg(rc);
        }
        Some("zsh") => install_zsh(command, &files.join("zsh"), &seer)?,
        _ => {}
    }
    Ok(())
}

// A login bash reads no rcfile. In POSIX mode an interactive bash reads only
// the file in ENV, and --login still sets the login flag. Leaving POSIX mode
// keeps inherit_errexit on, so the file turns it off unless the user set it in
// BASHOPTS. bash 3.2 has no inherit_errexit. POSIX mode also moves the default
// HISTFILE to .sh_history.
fn install_bash_login(command: &mut CommandBuilder, files: &Path, seer: &Path) -> io::Result<()> {
    let path = files.join("bash_login");
    let body = format!(
        "set +o posix\n\
         case \":${{SEER_BASHOPTS-}}:\" in *:inherit_errexit:*) ;; *) shopt -u inherit_errexit 2>/dev/null ;; esac\n\
         unset SEER_BASHOPTS\n\
         if [ -n \"${{SEER_ENV+x}}\" ]; then export ENV=\"$SEER_ENV\"; unset SEER_ENV; else unset ENV; fi\n\
         if [ -n \"${{SEER_HISTFILE+x}}\" ]; then HISTFILE=\"$HOME/.bash_history\"; unset SEER_HISTFILE; fi\n\
         {BASH_LOGIN_FILES}exit() {{ {} exit; }}\n",
        posix_quote(seer)
    );
    write_private(files, &path, &body)?;
    if let Some(original) = command.get_env("ENV").map(ToOwned::to_owned) {
        command.env("SEER_ENV", original);
    }
    if let Some(options) = command.get_env("BASHOPTS").map(ToOwned::to_owned) {
        command.env("SEER_BASHOPTS", options);
    }
    if command.get_env("HISTFILE").is_none() {
        command.env("SEER_HISTFILE", "1");
    }
    command.env("ENV", path);
    command.args(["--login", "--posix"]);
    Ok(())
}

// zsh has no rcfile option. ZDOTDIR points at Seer's files, and each one
// loads the user file of the same name. zsh reads each file from the ZDOTDIR
// of that moment, so a user file runs with the user ZDOTDIR (unset, empty or
// set, exported or not) and Seer keeps any change it makes. Issue 501: macOS
// /etc/zshrc runs before the Seer .zshrc and sets HISTFILE from ZDOTDIR.
// After a user file, Seer sets ZDOTDIR again only if zsh will read the next
// Seer file: ZDOTDIR is not read only, RCS is set, and there is no sh or ksh
// emulation (zsh then reads the next files from HOME, see sourcehome in
// zsh Src/init.c). If not, Seer sets exit at once and keeps the user
// ZDOTDIR. The exit function is always set right after the user .zshrc.
// Code that a startup file runs has a ZSH_EVAL_CONTEXT that starts with
// file, so there exit stays the builtin with the status of the last command.
fn install_zsh(command: &mut CommandBuilder, directory: &Path, seer: &Path) -> io::Result<()> {
    let exit = format!(
        "exit() {{ case ${{ZSH_EVAL_CONTEXT-}} in file*) builtin exit \"${{1-$?}}\";; esac; {} exit; }}\n",
        posix_quote(seer)
    );
    let done = "unset seer_zdotdir seer_zdotdir_set seer_zdotdir_export\n";
    let restore = "unset ZDOTDIR\n\
         if [ -n \"$seer_zdotdir_set\" ]; then ZDOTDIR=$seer_zdotdir; \
         if [ -n \"$seer_zdotdir_export\" ]; then export ZDOTDIR; else typeset +x ZDOTDIR; fi; fi\n";
    let next = |test: &str, finish: &str| {
        format!(
            "case ${{(t)ZDOTDIR-}}:$(emulate) in *readonly*|*:sh|*:ksh) {finish};; *) if {test}; then\n\
             seer_zdotdir_set=; seer_zdotdir_export=; seer_zdotdir=\n\
             if [ -n \"${{ZDOTDIR+x}}\" ]; then seer_zdotdir_set=x; seer_zdotdir=$ZDOTDIR; \
             case ${{(t)ZDOTDIR}} in *export*) seer_zdotdir_export=x;; esac; fi\n\
             ZDOTDIR={}\nelse\n{finish}fi;; esac\n",
            posix_quote(directory)
        )
    };
    let early = next("[[ -o rcs ]]", &format!("{exit}{done}"));
    let files = [
        (
            ".zshenv",
            format!(
                "if [ -n \"${{SEER_ZDOTDIR+x}}\" ]; then ZDOTDIR=\"$SEER_ZDOTDIR\"; else unset ZDOTDIR; fi\n\
                 unset SEER_ZDOTDIR\n{}{early}",
                load_zsh(".zshenv")
            ),
        ),
        (
            ".zprofile",
            format!("{restore}{}{early}", load_zsh(".zprofile")),
        ),
        (
            ".zshrc",
            format!(
                "if [ \"${{HISTFILE-}}\" = {} ]; then HISTFILE=${{seer_zdotdir:-$HOME}}/.zsh_history; fi\n\
                 {restore}{}{exit}{}",
                posix_quote(&directory.join(".zsh_history")),
                load_zsh(".zshrc"),
                next("[[ -o login && -o rcs ]]", done)
            ),
        ),
        (
            ".zlogin",
            format!("{restore}{}{exit}{done}", load_zsh(".zlogin")),
        ),
    ];
    for (name, body) in files {
        write_private(directory, &directory.join(name), &body)?;
    }
    if let Some(original) = env::var_os("ZDOTDIR") {
        command.env("SEER_ZDOTDIR", original);
    }
    command.env("ZDOTDIR", directory);
    Ok(())
}

fn load_zsh(name: &str) -> String {
    format!("[ -f \"${{ZDOTDIR-$HOME}}/{name}\" ] && . \"${{ZDOTDIR-$HOME}}/{name}\"\n")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PtySession;
    use crate::pty::lock_mutex;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    const PROFILE: &str = "SEER_LOGIN_PROBE=profile; export SEER_LOGIN_PROBE\n";
    const BASH_PROBE: &str =
        "printf 'probe=%s-%s\\n' \"$(shopt -q login_shell && echo login)\" \"$SEER_LOGIN_PROBE\"\n";
    // POSIX sh has no login flag to read. Only a login sh reads .profile.
    const SH_PROBE: &str = "printf 'probe=login-%s\\n' \"$SEER_LOGIN_PROBE\"\n";
    const ZSH_PROBE: &str =
        "printf 'probe=%s-%s\\n' \"$([[ -o login ]] && echo login)\" \"$SEER_LOGIN_PROBE\"\n";
    const FISH_PROBE: &str =
        "printf 'probe=%s-%s\\n' (status is-login; and echo login) \"$SEER_LOGIN_PROBE\"\n";

    // Profile files can test the login flag, so a Seer login shell must set
    // it as well as read the profile files.
    #[test]
    fn each_login_shell_sets_the_login_flag_and_reads_its_profile() {
        let home = env::temp_dir().join(format!("seer-login-probe-{}", std::process::id()));
        write_profiles(&home);
        for (name, probe) in [
            ("bash", BASH_PROBE),
            ("sh", SH_PROBE),
            ("zsh", ZSH_PROBE),
            ("fish", FISH_PROBE),
        ] {
            let Some(program) = find_shell(name) else {
                eprintln!("{name} is not installed, skipped");
                continue;
            };
            let output = run_probe(program, &home, probe);
            assert!(output.contains("probe=login-profile"), "{name}: {output}");
        }
        let _ = fs::remove_dir_all(&home);
    }

    fn write_profiles(home: &Path) {
        let fish = home.join(".config/fish");
        fs::create_dir_all(&fish).expect("fish config directory must exist");
        for name in [".profile", ".bash_profile", ".zprofile"] {
            fs::write(home.join(name), PROFILE).expect("profile must be written");
        }
        fs::write(
            fish.join("config.fish"),
            "status is-login; and set -gx SEER_LOGIN_PROBE profile\n",
        )
        .expect("fish config must be written");
    }

    fn run_probe(program: String, home: &Path, probe: &str) -> String {
        let shell = Shell {
            program,
            login: true,
        };
        let mut command = shell.command();
        command.env("HOME", home);
        for name in [
            "XDG_CONFIG_HOME",
            "ZDOTDIR",
            "ENV",
            "HISTFILE",
            "SEER_LOGIN_PROBE",
        ] {
            command.env_remove(name);
        }
        install(&mut command, &shell).expect("shell files must be written");
        let seen = Arc::new(Mutex::new(String::new()));
        let reader = Arc::clone(&seen);
        let mut session = PtySession::start(command, 80, 24, move |bytes, answers| {
            let chunk = String::from_utf8_lossy(bytes);
            lock_mutex(&reader).push_str(&chunk);
            // fish waits for the reply to its device attributes query.
            if chunk.contains("\x1b[0c") {
                answers.send(b"\x1b[?62c".to_vec());
            }
        })
        .expect("shell must start");
        session
            .write_input(probe.as_bytes())
            .expect("probe must be sent");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !lock_mutex(&seen).contains("probe=login-profile") && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = lock_mutex(&seen).clone();
        let _ = session.kill();
        output
    }

    fn find_shell(name: &str) -> Option<String> {
        env::split_paths(&env::var_os("PATH")?)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
            .map(|path| path.to_string_lossy().into_owned())
    }
}

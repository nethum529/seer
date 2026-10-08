use std::env;
use std::ffi::{CStr, c_char};
use std::fs;
use std::mem::MaybeUninit;
use std::os::unix::fs::PermissionsExt;
use std::ptr;

use portable_pty::CommandBuilder;

pub const LOGIN_SHELL: &str = "login";

const PASSWD_BUFFER: usize = 16 * 1024;

pub(crate) struct Shell {
    pub(crate) program: String,
    pub(crate) login: bool,
}

// Issue 448: an app opened from the Dock or a desktop launcher has a small
// environment, so the shell must read the profile files like a terminal app
// does. The passwd entry is read for each new terminal, so a changed login
// shell applies to the next terminal.
pub(crate) fn resolve(configured: &str) -> Shell {
    if configured != LOGIN_SHELL {
        return Shell {
            program: configured.to_owned(),
            login: false,
        };
    }
    let program = account_shell()
        .or_else(|| env::var("SHELL").ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "/bin/sh".to_owned());
    Shell {
        program,
        login: true,
    }
}

impl Shell {
    pub(crate) fn command(&self) -> CommandBuilder {
        let mut command = CommandBuilder::new(&self.program);
        if self.login {
            command.env("SHELL", &self.program);
        }
        command
    }
}

fn account_shell() -> Option<String> {
    let mut entry = MaybeUninit::<libc::passwd>::uninit();
    let mut buffer = vec![0 as c_char; PASSWD_BUFFER];
    let mut found = ptr::null_mut();
    // SAFETY: every pointer is valid for the call, and the buffer length is
    // the real length. getpwuid_r writes only into entry and buffer.
    let status = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            entry.as_mut_ptr(),
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut found,
        )
    };
    if status != 0 || found.is_null() {
        return None;
    }
    // SAFETY: found points at entry, which getpwuid_r filled. pw_shell points
    // into buffer, which is still alive.
    let shell = unsafe { CStr::from_ptr((*found).pw_shell) }
        .to_str()
        .ok()?
        .to_owned();
    fs::metadata(&shell)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .then_some(shell)
}

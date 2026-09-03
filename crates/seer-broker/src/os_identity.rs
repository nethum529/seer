use std::ffi::c_int;
use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt, chown};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Clone)]
pub(crate) struct OsIdentity {
    name: String,
    uid: u32,
    gid: u32,
    groups: Vec<u32>,
    home: PathBuf,
    shell: PathBuf,
    inherit_process: bool,
}

impl OsIdentity {
    pub(crate) fn resolve(name: &str) -> io::Result<Self> {
        validate_account_name(name)?;
        let fields = account_fields(name)?;
        let uid = parse_id(&fields.uid, "user ID")?;
        let gid = parse_id(&fields.gid, "group ID")?;
        let groups = group_ids(name)?;
        let home = absolute_path(&fields.home, "home directory")?;
        let shell = absolute_path(&fields.shell, "login shell")?;
        Ok(Self {
            name: name.to_owned(),
            uid,
            gid,
            groups,
            home,
            shell,
            inherit_process: false,
        })
    }

    pub(crate) fn resolve_process_account() -> io::Result<Self> {
        let output = run_account_command("/usr/bin/id", &["-un"])?;
        if !output.status.success() {
            return Err(io::Error::other("id -un failed"));
        }
        let name = output_text(output)?;
        let mut identity = Self::resolve(name.trim())?;
        identity.inherit_process = true;
        Ok(identity)
    }

    pub(crate) const fn uid(&self) -> u32 {
        self.uid
    }

    pub(crate) fn shell(&self) -> &Path {
        &self.shell
    }

    pub(crate) fn prepare_directory(&self, path: &Path) -> io::Result<()> {
        let created = match DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => false,
            Err(error) => return Err(error),
        };
        if created && process_uid() != self.uid {
            chown(path, Some(self.uid), Some(self.gid))?;
        }
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_dir()
            || metadata.uid() != self.uid
            || metadata.gid() != self.gid
        {
            return Err(unsafe_directory(&self.name));
        }
        fs::set_permissions(path, Permissions::from_mode(0o700))
    }

    pub(crate) fn check_switch_rights(&self) -> io::Result<()> {
        if !self.inherit_process && self.needs_switch()? && process_uid() != 0 {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "broker cannot switch to OS account {}: insufficient process rights",
                    self.name
                ),
            ))
        } else {
            Ok(())
        }
    }

    pub(crate) fn apply(&self, command: &mut Command) -> io::Result<()> {
        if self.inherit_process {
            return Ok(());
        }
        command
            .env("HOME", &self.home)
            .env("USER", &self.name)
            .env("LOGNAME", &self.name)
            .env("SHELL", &self.shell)
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .env_remove("ZDOTDIR")
            .env_remove("XDG_CACHE_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_DATA_HOME")
            .env_remove("XDG_RUNTIME_DIR")
            .env_remove("XDG_STATE_HOME");

        let needs_switch = self.needs_switch()?;
        self.check_switch_rights()?;
        if !needs_switch {
            return Ok(());
        }
        let uid = self.uid;
        let gid = self.gid;
        let groups = self.groups.clone();
        // Safety: The child closure only calls credential syscalls with owned numeric IDs.
        unsafe {
            command.pre_exec(move || {
                set_supplementary_groups(&groups)?;
                syscall_result(setgid(gid))?;
                syscall_result(setuid(uid))
            });
        }
        Ok(())
    }

    fn needs_switch(&self) -> io::Result<bool> {
        if process_uid() != self.uid || process_gid() != self.gid {
            return Ok(true);
        }
        if process_uid() != 0 {
            // The broker already runs under the target account. A
            // broker without root rights cannot change its session
            // groups, so there is nothing left to switch.
            return Ok(false);
        }
        let mut current = current_groups()?;
        let mut target = self.groups.clone();
        current.sort_unstable();
        target.sort_unstable();
        Ok(current != target)
    }
}

struct AccountFields {
    uid: String,
    gid: String,
    home: String,
    shell: String,
}

#[cfg(target_os = "linux")]
fn account_fields(name: &str) -> io::Result<AccountFields> {
    let output = run_account_command("/usr/bin/getent", &["passwd", name])?;
    if !output.status.success() {
        return Err(account_not_found(name));
    }
    let record = output_text(output)?;
    let fields = record.trim().split(':').collect::<Vec<_>>();
    if fields.len() != 7 || fields[0] != name {
        return Err(invalid_account(name));
    }
    Ok(AccountFields {
        uid: fields[2].to_owned(),
        gid: fields[3].to_owned(),
        home: fields[5].to_owned(),
        shell: fields[6].to_owned(),
    })
}

#[cfg(target_os = "macos")]
fn account_fields(name: &str) -> io::Result<AccountFields> {
    let output = run_account_command("/usr/bin/dscacheutil", &["-q", "user", "-a", "name", name])?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err(account_not_found(name));
    }
    let record = output_text(output)?;
    Ok(AccountFields {
        uid: labeled_field(&record, "uid", name)?,
        gid: labeled_field(&record, "gid", name)?,
        home: labeled_field(&record, "dir", name)?,
        shell: labeled_field(&record, "shell", name)?,
    })
}

#[cfg(target_os = "macos")]
fn labeled_field(record: &str, label: &str, name: &str) -> io::Result<String> {
    record
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{label}: ")))
        .map(str::to_owned)
        .ok_or_else(|| invalid_account(name))
}

fn group_ids(name: &str) -> io::Result<Vec<u32>> {
    let output = run_account_command("/usr/bin/id", &["-G", name])?;
    if !output.status.success() {
        return Err(account_not_found(name));
    }
    parse_groups(&output_text(output)?)
}

fn current_groups() -> io::Result<Vec<u32>> {
    let output = run_account_command("/usr/bin/id", &["-G"])?;
    if !output.status.success() {
        return Err(io::Error::other("id -G failed"));
    }
    parse_groups(&output_text(output)?)
}

pub(crate) fn process_uid() -> u32 {
    // Safety: geteuid has no arguments and does not modify memory.
    unsafe { geteuid() }
}

fn process_gid() -> u32 {
    // Safety: getegid has no arguments and does not modify memory.
    unsafe { getegid() }
}

fn run_account_command(program: &str, arguments: &[&str]) -> io::Result<Output> {
    Command::new(program).args(arguments).output()
}

fn output_text(output: Output) -> io::Result<String> {
    String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn parse_groups(value: &str) -> io::Result<Vec<u32>> {
    value
        .split_whitespace()
        .map(|group| parse_id(group, "supplementary group ID"))
        .collect()
}

fn parse_id(value: &str, label: &str) -> io::Result<u32> {
    value.parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("OS account has an invalid {label}"),
        )
    })
}

fn absolute_path(value: &str, label: &str) -> io::Result<PathBuf> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("OS account has an invalid {label}"),
        ))
    }
}

fn validate_account_name(name: &str) -> io::Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "OS account name is unsafe",
        ))
    }
}

fn unsafe_directory(name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "runtime directory for OS account {name} is a symlink or has unsafe ownership"
        ),
    )
}

fn account_not_found(name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("OS account {name} was not found"),
    )
}

fn invalid_account(name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("OS account {name} has an invalid record"),
    )
}

fn syscall_result(result: c_int) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn set_supplementary_groups(groups: &[u32]) -> io::Result<()> {
    // Safety: The pointer is valid for the supplied slice length.
    syscall_result(unsafe { setgroups(groups.len(), groups.as_ptr()) })
}

#[cfg(target_os = "macos")]
fn set_supplementary_groups(groups: &[u32]) -> io::Result<()> {
    let count = c_int::try_from(groups.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "too many groups"))?;
    // Safety: The pointer is valid for the supplied slice length.
    syscall_result(unsafe { setgroups(count, groups.as_ptr()) })
}

unsafe extern "C" {
    fn geteuid() -> u32;
    fn getegid() -> u32;
    fn setuid(uid: u32) -> c_int;
    fn setgid(gid: u32) -> c_int;
}

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn setgroups(size: usize, list: *const u32) -> c_int;
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn setgroups(size: c_int, list: *const u32) -> c_int;
}

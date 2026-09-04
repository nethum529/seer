use std::env;
use std::ffi::OsString;
use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::lifecycle::validate_user_id;
use crate::os_identity::{OsIdentity, process_uid};

const MAX_SOCKET_PATH_BYTES: usize = 99;

pub(super) fn runtime_socket_path(
    user: &str,
    identity: &OsIdentity,
    state_dir: &Path,
) -> io::Result<PathBuf> {
    validate_user_id(user)?;
    let xdg_runtime_dir = env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty());
    let broker_uid = process_uid();
    let directory = runtime_directory_path(state_dir, xdg_runtime_dir, broker_uid, identity.uid());
    let path = directory.join(format!("{user}.sock"));
    validate_socket_path(&path)?;
    Ok(path)
}

pub(super) fn prepare_runtime_socket_path(path: &Path, identity: &OsIdentity) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("runtime socket directory has no parent"))?;
    let parent = directory
        .parent()
        .ok_or_else(|| io::Error::other("runtime socket directory has no parent"))?;
    if !parent.exists() {
        create_private_directory(parent)?;
    }
    identity.prepare_directory(directory)
}

pub(super) fn allow_identity_traversal(directory: &Path, identity: &OsIdentity) -> io::Result<()> {
    let metadata = fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink() {
        return Err(unsafe_directory(directory));
    }
    if metadata.uid() != identity.uid() {
        fs::set_permissions(directory, Permissions::from_mode(0o711))?;
    }
    Ok(())
}

fn unsafe_directory(directory: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("runtime directory is unsafe: {}", directory.display()),
    )
}

fn validate_socket_path(path: &Path) -> io::Result<()> {
    if path.as_os_str().as_bytes().len() > MAX_SOCKET_PATH_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime socket path exceeds 99 bytes",
        ));
    }
    Ok(())
}

fn runtime_directory_path(
    state_dir: &Path,
    xdg_runtime_dir: Option<OsString>,
    broker_uid: u32,
    target_uid: u32,
) -> PathBuf {
    match xdg_runtime_dir.filter(|value| !value.is_empty() && broker_uid == target_uid) {
        Some(root) => PathBuf::from(root).join("seer"),
        None => state_dir.join(format!("seer-{target_uid}")),
    }
}

pub(super) fn create_private_directory(directory: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true).mode(0o700).create(directory)?;
    fs::set_permissions(directory, Permissions::from_mode(0o700))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::validate_socket_path;

    #[test]
    fn limits_socket_paths_to_less_than_one_hundred_bytes() {
        let allowed = Path::new("a").join("b".repeat(97));
        let rejected = Path::new("a").join("b".repeat(98));

        assert_eq!(allowed.as_os_str().len(), 99);
        validate_socket_path(&allowed).expect("99-byte socket path must be valid");
        assert_eq!(rejected.as_os_str().len(), 100);
        assert_eq!(
            validate_socket_path(&rejected)
                .expect_err("100-byte socket path must be invalid")
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
}

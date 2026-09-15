use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// Issue 362: the installer must replace a binary that is still running. On
// macOS a copy over the old file keeps the inode, and the kernel then kills
// the new binary for an invalid signature. On Linux the copy fails with
// ETXTBSY. Both need a new inode.
#[test]
fn installing_over_a_running_runtime_replaces_the_file() {
    let root = PathBuf::from(format!("/tmp/seer-install-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let bin = root.join(".local/bin");
    fs::create_dir_all(&bin).expect("test directory must be created");
    let old_runtime = bin.join("seer-runtime");
    fs::copy("/bin/sleep", &old_runtime).expect("old runtime must be copied");
    let mut running = Command::new(&old_runtime)
        .arg("60")
        .spawn()
        .expect("old runtime must start");
    let old_inode = inode(&old_runtime);
    let release = write_release(&root);

    let output = Command::new("sh")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/install.sh"
        ))
        .env("HOME", &root)
        .env("SEER_INSTALL_URL", format!("file://{}", release.display()))
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .stdin(Stdio::null())
        .output()
        .expect("install.sh must run");
    let text = String::from_utf8_lossy(&output.stderr).into_owned();
    let installed_inode = inode(&old_runtime);
    let still_running = running.try_wait().expect("old runtime status").is_none();
    let _ = running.kill();
    let _ = running.wait();
    let _ = fs::remove_dir_all(&root);

    assert!(output.status.success(), "{text}");
    assert_ne!(
        installed_inode, old_inode,
        "the installed file must be a new file"
    );
    assert!(still_running, "the old runtime must keep running");
}

fn inode(path: &Path) -> u64 {
    fs::metadata(path).expect("file must exist").ino()
}

fn write_release(root: &Path) -> PathBuf {
    let os = if std::env::consts::OS == "macos" {
        "darwin"
    } else {
        std::env::consts::OS
    };
    let arch = if std::env::consts::ARCH == "aarch64" {
        "arm64"
    } else {
        std::env::consts::ARCH
    };
    let unpacked = root.join("unpacked");
    let release = root.join("release");
    fs::create_dir_all(&unpacked).expect("unpacked directory must be created");
    fs::create_dir_all(&release).expect("release directory must be created");
    for binary in ["seer", "seer-broker", "seer-runtime"] {
        fs::write(unpacked.join(binary), format!("#!/bin/sh\necho {binary}\n"))
            .expect("release binary must be written");
    }
    let status = Command::new("tar")
        .arg("-C")
        .arg(&unpacked)
        .arg("-czf")
        .arg(release.join(format!("seer-{os}-{arch}.tar.gz")))
        .args(["seer", "seer-broker", "seer-runtime"])
        .status()
        .expect("tar must run");
    assert!(status.success(), "the release archive must be created");
    release
}

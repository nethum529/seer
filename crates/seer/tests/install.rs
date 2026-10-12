use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
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
    if cfg!(target_os = "macos") {
        // macOS rejects the Apple platform signature on a copied system binary.
        let status = Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(&old_runtime)
            .status()
            .expect("the copied fixture must be signed");
        assert!(status.success());
    }
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
        .env("XDG_DATA_HOME", root.join(".local/share"))
        .env("XDG_CONFIG_HOME", root.join(".config"))
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
    for binary in ["seer", "seer-broker", "seer-runtime", "seer-desktop"] {
        fs::write(unpacked.join(binary), format!("#!/bin/sh\necho {binary}\n"))
            .expect("release binary must be written");
        fs::set_permissions(unpacked.join(binary), fs::Permissions::from_mode(0o755))
            .expect("release binary must be executable");
    }
    fs::write(unpacked.join("version"), env!("CARGO_PKG_VERSION"))
        .expect("release version must be written");
    write_desktop_payload(&unpacked);
    let status = Command::new("tar")
        .arg("-C")
        .arg(&unpacked)
        .arg("-czf")
        .arg(release.join(format!("seer-{os}-{arch}.tar.gz")))
        .arg(".")
        .status()
        .expect("tar must run");
    assert!(status.success(), "the release archive must be created");
    release
}

fn write_desktop_payload(unpacked: &Path) {
    if cfg!(target_os = "macos") {
        let app = unpacked.join("Seer.app/Contents");
        fs::create_dir_all(app.join("MacOS")).expect("app binaries directory");
        fs::create_dir_all(app.join("Resources")).expect("app resources directory");
        for binary in ["seer", "seer-broker", "seer-runtime", "seer-desktop"] {
            fs::copy(unpacked.join(binary), app.join("MacOS").join(binary)).expect("bundle binary");
        }
        let plist = include_str!("../../../scripts/desktop/Info.plist")
            .replace("@VERSION@", env!("CARGO_PKG_VERSION"));
        fs::write(app.join("Info.plist"), plist).expect("bundle metadata");
        fs::write(app.join("Resources/Seer.icns"), b"fixture icon").expect("bundle icon");
    } else {
        let icons = unpacked.join("share/icons/hicolor/scalable/apps");
        fs::create_dir_all(&icons).expect("icon directory");
        fs::write(
            icons.join("seer.svg"),
            include_bytes!("../../../scripts/desktop/seer.svg"),
        )
        .expect("desktop icon");
    }
}

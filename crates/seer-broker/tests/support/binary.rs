use std::path::{Path, PathBuf};
use std::process::Command;

pub fn build(binary: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["build", "-p", "seer", "--bin", binary])
        .current_dir(manifest)
        .status()
        .expect("sibling binary must build");
    assert!(status.success(), "sibling binary must build");
    manifest.join("../../target/debug").join(binary)
}

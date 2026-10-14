use std::process::Command;

#[test]
fn join_opens_the_overlay_in_the_same_terminal() {
    check("direct");
}

#[test]
fn install_join_opens_the_overlay_in_the_same_terminal() {
    check("install");
}

fn check(path: &str) {
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/tests/join_overlay.py"
        ))
        .args([
            env!("CARGO_BIN_EXE_seer"),
            env!("CARGO_BIN_EXE_seer-runtime"),
            path,
        ])
        .output()
        .expect("the join e2e test must run");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

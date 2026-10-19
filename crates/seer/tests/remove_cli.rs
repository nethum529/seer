#[path = "support/cli_run.rs"]
mod cli_run;

use cli_run::{TestConfig, run, text};

#[test]
fn remove_requires_one_person_and_is_listed_in_help() {
    let config = TestConfig::new();
    let help = run(&config, &["help"], "");
    assert!(help.status.success());
    assert!(text(&help.stdout).contains("remove <person>"));
    for arguments in [vec!["remove"], vec!["remove", "Guest", "bob"]] {
        let output = run(&config, &arguments, "");
        assert_eq!(output.status.code(), Some(2));
    }
}

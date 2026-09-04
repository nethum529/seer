use std::fs;
use std::path::Path;

pub(crate) fn read_owner_identity(path: &Path) -> Option<(String, String)> {
    let output = fs::read_to_string(path).ok()?;
    let user_id = output
        .lines()
        .find_map(|line| line.strip_prefix("owner-id: "))?;
    let credential = output
        .lines()
        .find_map(|line| line.strip_prefix("owner-credential: "))?;
    Some((user_id.to_owned(), credential.to_owned()))
}

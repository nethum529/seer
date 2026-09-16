/// Reads the major and minor numbers of a Seer version such as "0.6.0".
pub fn major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.trim().split(".");
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// The server and client versions in a room refusal for another major.minor
/// version. Every room server since 2026-09-01 (0.1.0) refuses with
/// "version mismatch: server X, client Y. Run: seer update".
pub fn version_mismatch(reason: &str) -> Option<(&str, &str)> {
    let (_, rest) = reason.split_once("version mismatch: server ")?;
    let (server, rest) = rest.split_once(", client ")?;
    let client = rest.split_whitespace().next()?.trim_end_matches('.');
    (major_minor(server)? != major_minor(client)?).then_some((server, client))
}

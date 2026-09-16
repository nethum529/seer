/// Reads the major and minor numbers of a Seer version such as "0.6.0".
pub fn major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.trim().split(".");
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

use std::fmt;

const PREFIX: &str = "SEER1-";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Capsule {
    pub(crate) endpoint: String,
    pub(crate) alias: String,
    pub(crate) token: String,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CapsuleError;

impl fmt::Display for CapsuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid invitation")
    }
}

impl std::error::Error for CapsuleError {}

pub(crate) fn parse(value: &str) -> Result<Capsule, CapsuleError> {
    let value = value.trim();
    let body = value.strip_prefix(PREFIX).ok_or(CapsuleError)?;
    let parts: Vec<&str> = body.split('-').collect();
    let port_index = parts
        .iter()
        .enumerate()
        .skip(1)
        .take(parts.len().saturating_sub(2))
        .find(|(_, part)| valid_port(part))
        .map(|(index, _)| index)
        .ok_or(CapsuleError)?;
    let host = parts[..port_index].join("-");
    let port = parts[port_index];
    let token = parts[port_index + 1..].join("-");
    validate(&host, &token)?;

    Ok(Capsule {
        endpoint: format!("{host}:{port}"),
        alias: host,
        token,
    })
}

fn validate(host: &str, token: &str) -> Result<(), CapsuleError> {
    if host.is_empty()
        || token.is_empty()
        || host.chars().any(char::is_whitespace)
        || token.chars().any(char::is_whitespace)
    {
        return Err(CapsuleError);
    }
    Ok(())
}

fn valid_port(value: &&str) -> bool {
    value.parse::<u16>().is_ok_and(|port| port > 0)
}

#[cfg(test)]
mod tests {
    use super::{Capsule, parse};

    #[test]
    fn parses_a_capsule_with_a_hyphenated_host() {
        assert_eq!(
            parse(" SEER1-team-west.example.com-7321-seat123\n"),
            Ok(Capsule {
                endpoint: "team-west.example.com:7321".into(),
                alias: "team-west.example.com".into(),
                token: "seat123".into(),
            })
        );
        assert_eq!(
            parse("SEER1-host-7321-seat-token")
                .expect("hyphenated token must parse")
                .token,
            "seat-token"
        );
    }

    #[test]
    fn rejects_invalid_capsules() {
        for value in [
            "MXS1-host-7321-token",
            "SEER1-host-token",
            "SEER1--7321-token",
            "SEER1-host-0-token",
            "SEER1-host-65536-token",
            "SEER1-host-port-token",
            "SEER1-host-7321-",
            "SEER1-bad host-7321-token",
            "SEER1-host-7321-bad token",
        ] {
            assert!(parse(value).is_err(), "{value} must be rejected");
        }
        assert_eq!(super::CapsuleError.to_string(), "invalid invitation");
    }
}

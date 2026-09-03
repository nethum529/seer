use std::fmt;

const TCP_PREFIX: &str = "SEER1-";
const IROH_PREFIX: &str = "SEER2-";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Endpoint {
    Tcp(String),
    Iroh(String),
}

impl fmt::Display for Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp(endpoint) => formatter.write_str(endpoint),
            Self::Iroh(key) => write!(formatter, "iroh:{key}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Capsule {
    pub(crate) endpoint: Endpoint,
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
    if let Some(body) = value.strip_prefix(TCP_PREFIX) {
        return parse_tcp(body);
    }
    if value.starts_with(IROH_PREFIX) {
        return parse_iroh(value);
    }
    Err(CapsuleError)
}

pub(crate) fn format_iroh(key: &str, token: &str) -> String {
    format!("{IROH_PREFIX}{key}-{token}")
}

pub(crate) fn parse_endpoint(value: &str) -> Option<Endpoint> {
    if let Some(key) = value.strip_prefix("iroh:") {
        return valid_iroh_key(key).then(|| Endpoint::Iroh(key.into()));
    }
    valid_tcp_endpoint(value).then(|| Endpoint::Tcp(value.into()))
}

fn parse_tcp(body: &str) -> Result<Capsule, CapsuleError> {
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
        endpoint: Endpoint::Tcp(format!("{host}:{port}")),
        alias: host,
        token,
    })
}

fn parse_iroh(value: &str) -> Result<Capsule, CapsuleError> {
    let body = value.strip_prefix(IROH_PREFIX).ok_or(CapsuleError)?;
    let (key, token) = body.split_once('-').ok_or(CapsuleError)?;
    if format_iroh(key, token) != value {
        return Err(CapsuleError);
    }
    validate(key, token)?;
    let endpoint = parse_endpoint(&format!("iroh:{key}")).ok_or(CapsuleError)?;

    Ok(Capsule {
        endpoint,
        alias: key[..8].into(),
        token: token.into(),
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

fn valid_port(value: &str) -> bool {
    value.parse::<u16>().is_ok_and(|port| port > 0)
}

fn valid_iroh_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn valid_tcp_endpoint(value: &str) -> bool {
    value.rsplit_once(':').is_some_and(|(host, port)| {
        !host.is_empty() && !host.chars().any(char::is_whitespace) && valid_port(port)
    })
}

#[cfg(test)]
mod tests {
    use super::{Capsule, Endpoint, format_iroh, parse, parse_endpoint};

    const IROH_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn parses_a_capsule_with_a_hyphenated_host() {
        assert_eq!(
            parse(" SEER1-team-west.example.com-7321-seat123\n"),
            Ok(Capsule {
                endpoint: Endpoint::Tcp("team-west.example.com:7321".into()),
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
        assert_eq!(
            parse(&format_iroh(IROH_KEY, "seat-token")),
            Ok(Capsule {
                endpoint: Endpoint::Iroh(IROH_KEY.into()),
                alias: "01234567".into(),
                token: "seat-token".into(),
            })
        );

        let endpoint = Endpoint::Iroh(IROH_KEY.into());
        assert_eq!(parse_endpoint(&endpoint.to_string()), Some(endpoint));
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
            "SEER2-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde-token",
            "SEER2-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeF-token",
            "SEER2-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg-token",
            "SEER2-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef-",
        ] {
            assert!(parse(value).is_err(), "{value} must be rejected");
        }
        assert_eq!(super::CapsuleError.to_string(), "invalid invitation");
    }
}

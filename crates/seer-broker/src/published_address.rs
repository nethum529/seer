use std::io;

pub(crate) struct PublishedAddress {
    host: String,
    port: u16,
}

impl PublishedAddress {
    pub(crate) fn parse(value: &str) -> io::Result<Self> {
        let (host, port) = value.rsplit_once(':').ok_or_else(invalid_published_addr)?;
        if host.is_empty() {
            return Err(invalid_published_addr());
        }
        let port = port.parse().map_err(|_| invalid_published_addr())?;
        Ok(Self {
            host: host.to_owned(),
            port,
        })
    }

    pub(crate) fn capsule(&self, token: &str) -> String {
        format!("SEER1-{}-{}-{token}", self.host, self.port)
    }
}

fn invalid_published_addr() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "published_addr must contain a host and port",
    )
}

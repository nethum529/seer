use std::env;
use std::io;
use std::net::TcpListener;

mod attachments;
mod config;
mod connection_limit;
mod forwarding;
mod os_identity;
mod registry;
mod runtime;
mod server;
#[cfg(test)]
mod test_support;

pub use config::Config;
pub use server::serve;

pub fn run() -> io::Result<()> {
    let config_path = env::args_os().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: seer-broker <config-path>",
        )
    })?;
    let config = Config::load(config_path)?;
    let listener = TcpListener::bind(config.listen)?;
    serve(listener, &config)
}

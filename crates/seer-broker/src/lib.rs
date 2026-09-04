use std::env;
use std::io;
use std::net::TcpListener;

mod attachments;
mod config;
mod connection_limit;
mod forwarding;
mod lifecycle;
mod os_identity;
mod registry;
mod runtime;
mod runtime_paths;
mod runtime_supervisor;
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
    let remote_listener = server::bind_remote_listener(&config)?;
    let listener = TcpListener::bind(config.listen)?;
    serve(listener, remote_listener, &config)
}

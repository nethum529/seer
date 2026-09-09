use std::env;
use std::io::{self, Write};
use std::net::TcpListener;

mod attachments;
mod config;
mod connection_limit;
mod forwarding;
mod grants;
mod published_address;
mod publishing;
mod registry;
mod runtime;
mod server;
#[cfg(test)]
mod test_support;

pub use config::Config;
pub use server::serve;

pub fn run() -> io::Result<()> {
    let mut arguments = env::args_os().skip(1);
    let config_path = arguments.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: seer-broker <config-path> [--remint-owner]",
        )
    })?;
    let remint = match arguments.next() {
        None => false,
        Some(flag) if flag == "--remint-owner" && arguments.next().is_none() => true,
        Some(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: seer-broker <config-path> [--remint-owner]",
            ));
        }
    };
    let config = Config::load(config_path)?;
    if remint {
        let (registry, _) = registry::Registry::open(&config.state_dir, &config.owner_name)?;
        let (user_id, credential) = registry.remint_owner()?;
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "owner-id: {user_id}")?;
        writeln!(stdout, "owner-credential: {credential}")?;
        return Ok(());
    }
    let remote_listener = server::bind_remote_listener(&config)?;
    let listener = TcpListener::bind(config.listen)?;
    serve(listener, remote_listener, &config)
}

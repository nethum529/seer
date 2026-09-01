use std::env;
use std::io;
use std::net::TcpListener;

use mux_broker::Config;

fn main() -> io::Result<()> {
    let config_path = env::args_os().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: mux-broker <config-path>",
        )
    })?;
    let config = Config::load(config_path)?;
    let listener = TcpListener::bind(config.listen)?;
    mux_broker::serve(listener, &config)
}

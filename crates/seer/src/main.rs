use std::process::ExitCode;

mod capsule;
mod cli;
mod commands;
mod input;
mod prompt;
mod start;
mod state;
mod store;
mod tailscale;
mod tui;

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1).peekable();
    if arguments.peek().is_none() {
        cli::run_bare()
    } else {
        cli::run(arguments)
    }
}

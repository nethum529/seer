use std::process::ExitCode;

mod capsule;
mod cli;
mod commands;
mod input;
mod prompt;
mod start;
mod state;
mod store;
mod tui;

fn main() -> ExitCode {
    cli::run(std::env::args().skip(1))
}

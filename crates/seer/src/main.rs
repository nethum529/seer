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
mod update;

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    match arguments.next() {
        None => cli::run_bare(),
        Some(command) if command == "update" && arguments.next().is_none() => update::run(),
        Some(command) => cli::run(std::iter::once(command).chain(arguments)),
    }
}

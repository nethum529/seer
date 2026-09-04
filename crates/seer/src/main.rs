use std::process::ExitCode;

mod capsule;
mod cli;
mod commands;
mod input;
mod prompt;
mod render;
mod start;
mod state;
mod store;
mod terminal_cells;
mod terminal_session;
mod theme;
mod tui;
mod tui_navigation;
mod update;
mod viewer;

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    match arguments.next() {
        None => cli::run_bare(),
        Some(command) if command == "update" && arguments.next().is_none() => update::run(),
        Some(command) => cli::run(std::iter::once(command).chain(arguments)),
    }
}

#[cfg(test)]
mod screen_tests;

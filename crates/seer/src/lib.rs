use std::process::ExitCode;

mod capsule;
mod cli;
mod commands;
#[cfg(debug_assertions)]
mod debug;
mod desktop;
mod input;
mod local;
mod panels;
mod person_menu;
mod processes;
mod prompt;
mod render;
pub mod rooms;
mod routes;
mod start;
mod state;
mod store;
mod terminal_cells;
mod terminal_session;
mod theme;
mod tui;
mod tui_link;
mod tui_navigation;
mod tui_sync;
mod update;
mod version_skew;
mod viewer;

pub use commands::join::{Invitation, JoinError, system_user_name};
pub use local::attach;
pub use store::{ServerEntry, ServerStore};

pub fn connect_room(server: &ServerEntry) -> std::io::Result<seer_net::Socket> {
    let (room, _) =
        commands::authenticate(server).map_err(|error| std::io::Error::other(error.message))?;
    tui_link::prepare_room(&room)?;
    Ok(room)
}

pub fn run() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    match arguments.next() {
        None => cli::run_bare(),
        Some(command) if command == "update" && arguments.next().is_none() => update::run(),
        #[cfg(target_os = "macos")]
        Some(command) if command == "--install-exchange" => update::exchange(arguments),
        Some(command) => cli::run(std::iter::once(command).chain(arguments)),
    }
}

#[cfg(test)]
mod screen_tests;
#[cfg(test)]
mod tui_link_tests;

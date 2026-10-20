use std::process::ExitCode;

mod capsule;
mod cli;
mod commands;
#[cfg(debug_assertions)]
mod debug;
mod desktop;
mod input;
mod invitation;
mod local;
mod panels;
mod person_menu;
mod processes;
mod prompt;
mod render;
pub mod rooms;
mod routes;
mod settings;
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
pub use invitation::{IssuedInvite, create_invite};
pub use local::attach;
pub use settings::Settings;
pub use store::{ServerEntry, ServerStore};
pub use version_skew::VersionRefusal;

pub fn connect_room(server: &ServerEntry) -> std::io::Result<seer_net::Socket> {
    let room =
        match commands::hello(server).map_err(|error| std::io::Error::other(error.message))? {
            commands::Welcome::Accepted(room, _) => room,
            commands::Welcome::Refused(reason) => {
                return Err(match version_skew::version_refusal(&reason) {
                    Some(refusal) => std::io::Error::other(refusal),
                    None => std::io::Error::other(format!("refused: {reason}")),
                });
            }
        };
    tui_link::prepare_room(&room)?;
    Ok(room)
}

pub fn update() -> Result<(), String> {
    update::update()
        .map(|_| ())
        .map_err(|error| error.to_string())
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

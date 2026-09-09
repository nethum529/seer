use std::process::ExitCode;

use crate::store::ServerEntry;

pub(super) fn run() -> ExitCode {
    let Ok(server) = crate::commands::selected_server() else {
        // seer start --restore rebuilds a lost owner store, so the room server
        // on this computer must still be stoppable with no room selected.
        return report(false, stop_hosted_broker(None));
    };
    let runtime = stop_runtime(&server);
    let broker = stop_hosted_broker(Some(server.endpoint.as_str()));
    report(runtime, broker)
}

fn stop_runtime(server: &ServerEntry) -> bool {
    match crate::local::stop(&server.user_id) {
        Ok(stopped) => stopped,
        Err(error) => {
            eprintln!("could not stop your terminals: {error}");
            false
        }
    }
}

#[cfg(target_os = "macos")]
fn stop_hosted_broker(_endpoint: Option<&str>) -> bool {
    false
}

// Only the room hosted on this computer may be stopped here. A joined room
// runs on someone else's computer and a different local room must not be
// caught by it.
#[cfg(target_os = "linux")]
fn stop_hosted_broker(endpoint: Option<&str>) -> bool {
    super::stop::run_broker(endpoint)
}

fn report(runtime: bool, broker: bool) -> ExitCode {
    if runtime {
        println!("Your terminals on this computer stopped.");
    }
    if broker {
        println!("Room server stopped.");
    }
    if runtime || broker {
        return ExitCode::SUCCESS;
    }
    eprintln!("Nothing runs on this computer for this room.");
    ExitCode::FAILURE
}

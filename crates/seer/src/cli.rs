use std::process::ExitCode;

use crate::commands::{self, CommandError};

const HELP: &str = "seer opens the interface.\n\nUsage: seer <command>\n\nCommands:\n  start [--restore]   Start the server on this machine\n  stop                Stop the server\n  update              Replace the binaries with the latest release\n  invite [--hours N]  Create a join line for a friend\n  join [capsule]      Join a server with a pasted line\n  list                List saved servers and people\n  attach              Open people and terminals\n  detach              Detach this client\n  exit                Leave Seer from inside a Seer terminal\n  leave               Remove yourself from the room\n  perms --on|--off    Allow or refuse everyone access to your terminals\n  peek <person>       Open with this person selected\n  help                Show this help\n\nMain screen: type into the selected terminal. All keys go to it.\nUse the mouse to select terminals and open Seer controls.\nLeft click the top right control to pick the person you look at.\nRight click the same control for the session actions.\nUse the back row in session to return to the overview.\nSession: j/k select, enter open, esc close, n new, x close, q quit.\nPerson menu: right click a person row. j/k select, enter watch,\nspace grant, esc close.\nFirst run: right click the top right control to copy the invite.\n";

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Bare,
    Help,
    Start(bool),
    Stop,
    Invite(Option<String>),
    Join,
    JoinWithInvitation(String),
    List,
    Attach,
    Detach,
    Exit,
    Leave,
    Perms(bool),
    Peek(String),
}

enum ParseError {
    Unknown(String),
    Usage,
}

pub(crate) fn run(arguments: impl Iterator<Item = String>) -> ExitCode {
    let command = match parse(arguments) {
        Ok(command) => command,
        Err(error) => {
            match error {
                ParseError::Unknown(name) => eprintln!("unknown command: {name}. Run seer help."),
                ParseError::Usage => eprint!("{HELP}"),
            }
            return ExitCode::from(2);
        }
    };
    execute(command)
}

pub(crate) fn run_bare() -> ExitCode {
    execute(Command::Bare)
}

fn execute(command: Command) -> ExitCode {
    let result = match command {
        Command::Bare => match commands::attach_bare() {
            Err(error) if error.message == "run seer join first" => {
                eprintln!("Paste the line the owner sent you.\nseer start creates the owner entry");
                return ExitCode::from(2);
            }
            result => result,
        },
        Command::Help => {
            print!("{HELP}");
            Ok(())
        }
        Command::Start(restore) => return crate::start::run(restore),
        Command::Stop => commands::stop(),
        Command::Invite(hours) => commands::invite(hours.as_deref()),
        Command::Join => commands::join(None),
        Command::JoinWithInvitation(invitation) => commands::join(Some(&invitation)),
        Command::List => commands::list(),
        Command::Attach => commands::attach(),
        Command::Detach => commands::detach(),
        Command::Exit => commands::exit(),
        Command::Leave => commands::leave(),
        Command::Perms(on) => commands::perms(on),
        Command::Peek(person) => commands::peek(&person),
    };
    finish(result)
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Command, ParseError> {
    let first = arguments.next().ok_or(ParseError::Usage)?;
    let command = match first.as_str() {
        "help" | "--help" | "-h" => Command::Help,
        "start" => match arguments.next() {
            None => Command::Start(false),
            Some(flag) if flag == "--restore" => Command::Start(true),
            Some(_) => return Err(ParseError::Usage),
        },
        "stop" => Command::Stop,
        "invite" => match arguments.next() {
            None => Command::Invite(None),
            Some(flag) if flag == "--hours" => {
                let hours = arguments.next().unwrap_or_default();
                if arguments.next().is_some() {
                    return Err(ParseError::Usage);
                }
                Command::Invite(Some(hours))
            }
            Some(_) => return Err(ParseError::Usage),
        },
        "join" => {
            let line = arguments.by_ref().collect::<Vec<_>>().join(" ");
            if line.is_empty() {
                Command::Join
            } else {
                Command::JoinWithInvitation(line)
            }
        }
        "list" => Command::List,
        "attach" => Command::Attach,
        "detach" => Command::Detach,
        "exit" => Command::Exit,
        "leave" => Command::Leave,
        "perms" => match arguments.next().as_deref() {
            Some("--on") => Command::Perms(true),
            Some("--off") => Command::Perms(false),
            _ => return Err(ParseError::Usage),
        },
        "peek" => {
            let person = arguments.next().ok_or(ParseError::Usage)?;
            if arguments.next().is_some() {
                return Err(ParseError::Usage);
            }
            Command::Peek(person)
        }
        _ => return Err(ParseError::Unknown(first)),
    };
    if arguments.next().is_some() {
        return Err(ParseError::Usage);
    }
    Ok(command)
}

fn finish(result: Result<(), CommandError>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error.message);
            ExitCode::from(error.code)
        }
    }
}

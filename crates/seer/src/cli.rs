use std::process::ExitCode;

use crate::commands::{self, CommandError};

const HELP: &str = "Usage: seer <command>\n\nCommands:\n  start          Start the server\n  stop           Stop the server\n  invite [--hours N]\n                 Create an invitation\n  join [capsule] Join a server\n  list           List saved servers and people\n  attach         Open people and terminals\n  detach         Detach this client\n  peek <person>  Open with this person selected\n\nBare seer opens people and terminals.\n\nMain screen: j/k people, h/l boxes, enter view, n new terminal, / search, esc back, q quit.\nViewer: esc back, tab next terminal, q quit.\nPerson menu: j/k select, enter watch, space grant, esc close.\nFirst run: c copy invite; n new terminal.\n";

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Bare,
    Help,
    Start,
    Stop,
    Invite(Option<String>),
    Join,
    JoinWithInvitation(String),
    List,
    Attach,
    Detach,
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
        Command::Start => return crate::start::run(),
        Command::Stop => return crate::start::stop(),
        Command::Invite(hours) => commands::invite(hours.as_deref()),
        Command::Join => commands::join(None),
        Command::JoinWithInvitation(invitation) => commands::join(Some(&invitation)),
        Command::List => commands::list(),
        Command::Attach => commands::attach(),
        Command::Detach => commands::detach(),
        Command::Peek(person) => commands::peek(&person),
    };
    finish(result)
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Command, ParseError> {
    let first = arguments.next().ok_or(ParseError::Usage)?;
    let command = match first.as_str() {
        "help" | "--help" | "-h" => Command::Help,
        "start" => Command::Start,
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

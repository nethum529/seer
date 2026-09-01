use std::process::ExitCode;

use crate::commands::{self, CommandError};

const HELP: &str = "Usage: seer <command>\n\nCommands:\n  start          Start the server\n  invite         Create an invitation\n  join [capsule] Join a server\n  list           List saved servers and people\n  attach         Attach to your tree\n  detach         Detach this client\n  peek <person>  View another person's tree\n";

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Help,
    Start,
    Invite,
    Join,
    JoinWithInvitation(String),
    List,
    Attach,
    Detach,
    Peek(String),
}

pub(crate) fn run(arguments: impl Iterator<Item = String>) -> ExitCode {
    let command = match parse(arguments) {
        Ok(command) => command,
        Err(()) => {
            eprint!("{HELP}");
            return ExitCode::from(2);
        }
    };
    let result = match command {
        Command::Help => {
            print!("{HELP}");
            Ok(())
        }
        Command::Start => return crate::start::run(),
        Command::Invite => commands::invite(),
        Command::Join => commands::join(None),
        Command::JoinWithInvitation(invitation) => commands::join(Some(&invitation)),
        Command::List => commands::list(),
        Command::Attach => commands::attach(),
        Command::Detach => commands::detach(),
        Command::Peek(person) => commands::peek(&person),
    };
    finish(result)
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Command, ()> {
    let first = arguments.next().ok_or(())?;
    let command = match first.as_str() {
        "--help" | "-h" if arguments.next().is_none() => Command::Help,
        "start" if arguments.next().is_none() => Command::Start,
        "invite" if arguments.next().is_none() => Command::Invite,
        "join" => match arguments.next() {
            None => Command::Join,
            Some(invitation) if arguments.next().is_none() => {
                Command::JoinWithInvitation(invitation)
            }
            Some(_) => return Err(()),
        },
        "list" if arguments.next().is_none() => Command::List,
        "attach" if arguments.next().is_none() => Command::Attach,
        "detach" if arguments.next().is_none() => Command::Detach,
        "peek" => {
            let person = arguments.next().ok_or(())?;
            if arguments.next().is_some() {
                return Err(());
            }
            Command::Peek(person)
        }
        _ => return Err(()),
    };
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

#[cfg(test)]
mod tests {
    use super::{Command, parse};

    #[test]
    fn parses_all_commands() {
        let cases = [
            (vec!["--help"], Command::Help),
            (vec!["start"], Command::Start),
            (vec!["invite"], Command::Invite),
            (vec!["join"], Command::Join),
            (vec!["list"], Command::List),
            (vec!["attach"], Command::Attach),
            (vec!["detach"], Command::Detach),
            (vec!["peek", "alice"], Command::Peek("alice".into())),
        ];

        for (arguments, expected) in cases {
            assert_eq!(parse(strings(&arguments)), Ok(expected));
        }
    }

    #[test]
    fn rejects_missing_and_extra_arguments() {
        for arguments in [
            Vec::new(),
            vec!["unknown"],
            vec!["join", "extra"],
            vec!["peek"],
            vec!["peek", "alice", "extra"],
            vec!["--help", "extra"],
        ] {
            assert_eq!(parse(strings(&arguments)), Err(()));
        }
    }

    fn strings(arguments: &[&str]) -> impl Iterator<Item = String> {
        arguments.iter().map(|argument| (*argument).to_owned())
    }
}

use std::env;
use std::io::{self, IsTerminal};
use std::net::TcpStream;
use std::process::ExitCode;

use seer_core::proto::{ClientMsg, ServerMsg, codec};

mod input;
mod state;
mod tui;

struct Arguments {
    addr: String,
    user: String,
    token: String,
    peek: Option<String>,
}

fn main() -> ExitCode {
    let Some(arguments) = parse_arguments(env::args().skip(1)) else {
        eprintln!("usage: seer-client <addr> <user> <token> [--peek <target-user>]");
        return ExitCode::from(2);
    };

    match connect(&arguments) {
        Ok((stream, ServerMsg::Welcome { user, tree })) => {
            if io::stdout().is_terminal() {
                tui::set_view_only(arguments.peek.is_some());
                run_tui(stream, tree)
            } else {
                println!("connected as {user}");
                ExitCode::SUCCESS
            }
        }
        Ok((_, ServerMsg::Refused { reason })) => {
            eprintln!("refused: {reason}");
            ExitCode::FAILURE
        }
        Ok((_, _)) => {
            eprintln!("error: unexpected server reply");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn parse_arguments(mut arguments: impl Iterator<Item = String>) -> Option<Arguments> {
    let addr = arguments.next()?;
    let user = arguments.next()?;
    let token = arguments.next()?;
    let peek = match (arguments.next(), arguments.next()) {
        (None, None) => None,
        (Some(flag), Some(target)) if flag == "--peek" => Some(target),
        _ => return None,
    };

    if arguments.next().is_some() {
        return None;
    }

    Some(Arguments {
        addr,
        user,
        token,
        peek,
    })
}

fn connect(arguments: &Arguments) -> io::Result<(TcpStream, ServerMsg)> {
    let mut stream = TcpStream::connect(&arguments.addr)?;
    let hello = ClientMsg::Hello {
        user: arguments.user.clone(),
        token: arguments.token.clone(),
    };
    codec::encode(&mut stream, &hello)?;
    let reply = codec::decode(&mut stream)?;
    if let (ServerMsg::Welcome { .. }, Some(target)) = (&reply, &arguments.peek) {
        codec::encode(
            &mut stream,
            &ClientMsg::Peek {
                user: target.clone(),
                workspace: "w1".into(),
            },
        )?;
    }
    Ok((stream, reply))
}

fn run_tui(stream: TcpStream, tree: seer_core::Tree) -> ExitCode {
    match tui::run(stream, tree) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_arguments;

    #[test]
    fn accepts_optional_peek_target() {
        let arguments =
            parse(&["addr", "alice", "token", "--peek", "bob"]).expect("peek arguments must parse");

        assert_eq!(arguments.addr, "addr");
        assert_eq!(arguments.user, "alice");
        assert_eq!(arguments.token, "token");
        assert_eq!(arguments.peek.as_deref(), Some("bob"));
    }

    #[test]
    fn accepts_arguments_without_peek() {
        let arguments = parse(&["addr", "alice", "token"]).expect("standard arguments must parse");

        assert_eq!(arguments.peek, None);
    }

    #[test]
    fn rejects_bare_peek_flag() {
        assert!(parse(&["addr", "alice", "token", "--peek"]).is_none());
    }

    #[test]
    fn rejects_other_extra_arguments() {
        assert!(parse(&["addr", "alice", "token", "extra"]).is_none());
        assert!(parse(&["addr", "alice", "token", "--other", "bob"]).is_none());
        assert!(parse(&["addr", "alice", "token", "--peek", "bob", "extra"]).is_none());
    }

    fn parse(arguments: &[&str]) -> Option<super::Arguments> {
        parse_arguments(arguments.iter().map(|argument| (*argument).to_owned()))
    }
}

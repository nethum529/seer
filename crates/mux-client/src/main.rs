use std::env;
use std::io::{self, IsTerminal};
use std::net::TcpStream;
use std::process::ExitCode;

use mux_core::proto::{ClientMsg, ServerMsg, codec};

mod input;
mod state;
mod tui;

struct Arguments {
    addr: String,
    user: String,
    token: String,
}

fn main() -> ExitCode {
    let Some(arguments) = parse_arguments() else {
        eprintln!("usage: mux-client <addr> <user> <token>");
        return ExitCode::from(2);
    };

    match connect(arguments) {
        Ok((stream, ServerMsg::Welcome { user, tree })) => {
            if io::stdout().is_terminal() {
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

fn parse_arguments() -> Option<Arguments> {
    let mut arguments = env::args().skip(1);
    let addr = arguments.next()?;
    let user = arguments.next()?;
    let token = arguments.next()?;

    if arguments.next().is_some() {
        return None;
    }

    Some(Arguments { addr, user, token })
}

fn connect(arguments: Arguments) -> io::Result<(TcpStream, ServerMsg)> {
    let mut stream = TcpStream::connect(arguments.addr)?;
    let hello = ClientMsg::Hello {
        user: arguments.user,
        token: arguments.token,
    };
    codec::encode(&mut stream, &hello)?;
    let reply = codec::decode(&mut stream)?;
    Ok((stream, reply))
}

fn run_tui(stream: TcpStream, tree: mux_core::Tree) -> ExitCode {
    match tui::run(stream, tree) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

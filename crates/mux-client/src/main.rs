use std::env;
use std::io;
use std::net::TcpStream;
use std::process::ExitCode;

use mux_core::proto::{ClientMsg, ServerMsg, codec};

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

    match exchange_hello(arguments) {
        Ok(ServerMsg::Welcome { user, .. }) => {
            println!("connected as {user}");
            ExitCode::SUCCESS
        }
        Ok(ServerMsg::Refused { reason }) => {
            eprintln!("refused: {reason}");
            ExitCode::FAILURE
        }
        Ok(_) => {
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

fn exchange_hello(arguments: Arguments) -> io::Result<ServerMsg> {
    let mut stream = TcpStream::connect(arguments.addr)?;
    let hello = ClientMsg::Hello {
        user: arguments.user,
        token: arguments.token,
    };
    codec::encode(&mut stream, &hello)?;
    codec::decode(&mut stream)
}

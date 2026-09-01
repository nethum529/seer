use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};

use crate::forwarding::forward;
use crate::runtime::RuntimeManager;
use crate::{Config, UserConfig};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const INVALID_CREDENTIALS: &str = "invalid credentials";
const EXPECTED_HELLO: &str = "expected Hello";
const INVALID_MESSAGE: &str = "invalid message";

pub fn serve(listener: TcpListener, config: &Config) -> io::Result<()> {
    let broker = Arc::new(Broker::new(config)?);
    for connection in listener.incoming() {
        let stream = connection?;
        let broker = Arc::clone(&broker);
        thread::spawn(move || report_connection(handle_connection(stream, &broker)));
    }
    Ok(())
}

struct Broker {
    users: Vec<UserConfig>,
    runtimes: RuntimeManager,
}

impl Broker {
    fn new(config: &Config) -> io::Result<Self> {
        Ok(Self {
            users: config.users.clone(),
            runtimes: RuntimeManager::new(config.shell.clone())?,
        })
    }
}

fn report_connection(result: io::Result<()>) -> bool {
    if let Err(error) = result {
        eprintln!("broker connection error: {error}");
        true
    } else {
        false
    }
}

fn handle_connection(mut stream: TcpStream, broker: &Broker) -> io::Result<()> {
    let Some(user) = handshake(&mut stream, &broker.users, HANDSHAKE_TIMEOUT)? else {
        return Ok(());
    };
    stream.set_read_timeout(None)?;
    forward(stream, &user, &broker.users, &broker.runtimes)
}

fn handshake(
    stream: &mut TcpStream,
    users: &[UserConfig],
    timeout: Duration,
) -> io::Result<Option<String>> {
    let deadline = Instant::now() + timeout;
    let message = match read_message(stream, deadline) {
        Ok(message) => message,
        Err(error) if connection_was_silent(&error) => return Ok(None),
        Err(_) => return refuse(stream, INVALID_MESSAGE).map(|()| None),
    };

    match message {
        ClientMsg::Hello { user, token } => authenticate(stream, users, &user, &token),
        _ => refuse(stream, EXPECTED_HELLO).map(|()| None),
    }
}

fn read_message(stream: &mut TcpStream, deadline: Instant) -> io::Result<ClientMsg> {
    codec::decode(&mut DeadlineReader { stream, deadline })
}

fn authenticate(
    stream: &mut TcpStream,
    users: &[UserConfig],
    user: &str,
    token: &str,
) -> io::Result<Option<String>> {
    let valid = users
        .iter()
        .find(|entry| entry.user == user)
        .is_some_and(|entry| tokens_equal(&entry.token, token));

    if valid {
        codec::encode(
            stream,
            &ServerMsg::Welcome {
                user: user.to_owned(),
                tree: Tree::new(),
            },
        )?;
        Ok(Some(user.to_owned()))
    } else {
        refuse(stream, INVALID_CREDENTIALS).map(|()| None)
    }
}

fn refuse(stream: &mut TcpStream, reason: &str) -> io::Result<()> {
    eprintln!("refused connection: {reason}");
    codec::encode(
        stream,
        &ServerMsg::Refused {
            reason: reason.to_owned(),
        },
    )
}

fn connection_was_silent(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::UnexpectedEof
    )
}

fn tokens_equal(expected: &str, supplied: &str) -> bool {
    constant_time_eq::constant_time_eq(expected.as_bytes(), supplied.as_bytes())
}

struct DeadlineReader<'a> {
    stream: &'a mut TcpStream,
    deadline: Instant,
}

impl Read for DeadlineReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(handshake_timed_out)?;
        self.stream.set_read_timeout(Some(remaining))?;
        self.stream.read(buffer)
    }
}

fn handshake_timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "handshake timed out")
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    use super::{connection_was_silent, handshake, report_connection, tokens_equal};

    #[test]
    fn token_comparison_checks_all_bytes_and_length() {
        assert!(tokens_equal("secret", "secret"));
        assert!(!tokens_equal("secret", "secrex"));
        assert!(!tokens_equal("secret", "secret-extra"));
        assert!(!tokens_equal("secret", "sec"));
    }

    #[test]
    fn silent_connection_reaches_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
        let address = listener
            .local_addr()
            .expect("listener must have an address");
        let mut client = TcpStream::connect(address).expect("client must connect");
        let (mut server, _) = listener.accept().expect("server must accept client");

        let result = handshake(&mut server, &[], Duration::from_millis(10));
        drop(server);
        let mut byte = [0];
        let bytes_read = std::io::Read::read(&mut client, &mut byte)
            .expect("silent connection must close without a response");

        assert_eq!(result.expect("silent handshake must finish"), None);
        assert_eq!(bytes_read, 0);
    }

    #[test]
    fn identifies_silent_connection_errors() {
        for kind in [
            io::ErrorKind::TimedOut,
            io::ErrorKind::WouldBlock,
            io::ErrorKind::UnexpectedEof,
        ] {
            assert!(connection_was_silent(&io::Error::from(kind)));
        }
        assert!(!connection_was_silent(&io::Error::from(
            io::ErrorKind::InvalidData
        )));
    }

    #[test]
    fn reports_handshake_error() {
        assert!(report_connection(Err(io::Error::other("test error"))));
        assert!(!report_connection(Ok(())));
    }

    #[test]
    fn expired_deadline_returns_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
        let address = listener
            .local_addr()
            .expect("listener must have an address");
        let _client = TcpStream::connect(address).expect("client must connect");
        let (mut server, _) = listener.accept().expect("server must accept client");

        let error = super::read_message(&mut server, std::time::Instant::now())
            .expect_err("expired deadline must return an error");

        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn malformed_message_is_refused() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
        let address = listener
            .local_addr()
            .expect("listener must have an address");
        let mut client = TcpStream::connect(address).expect("client must connect");
        let (mut server, _) = listener.accept().expect("server must accept client");

        let worker = thread::spawn(move || handshake(&mut server, &[], Duration::from_secs(1)));
        std::io::Write::write_all(&mut client, &[0, 0, 0, 1, b'{'])
            .expect("invalid frame must send");
        let response =
            seer_core::proto::codec::decode::<_, seer_core::proto::ServerMsg>(&mut client)
                .expect("refusal must decode");

        assert_eq!(
            response,
            seer_core::proto::ServerMsg::Refused {
                reason: "invalid message".into()
            }
        );
        worker
            .join()
            .expect("worker must not panic")
            .expect("handshake must finish");
    }
}

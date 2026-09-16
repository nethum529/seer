use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, Person, PersonState, ServerMsg, codec};

use super::server_io::receive;

#[path = "cli_run.rs"]
mod cli_run;
pub(crate) use cli_run::{TestConfig, run, text};

pub(crate) fn listener() -> TcpListener {
    TcpListener::bind("127.0.0.1:0").expect("listener must bind")
}

pub(crate) fn accept(listener: &TcpListener) -> TcpStream {
    listener
        .set_nonblocking(true)
        .expect("listener must become nonblocking");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("accepted stream must become blocking");
                stream
                    .set_read_timeout(Some(Duration::from_millis(100)))
                    .expect("read timeout must be set");
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .expect("write timeout must be set");
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "server accept timed out");
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("server accept failed: {error}"),
        }
    }
}

pub(crate) fn assert_hello(stream: &mut impl Read) {
    assert_eq!(
        receive(stream),
        ClientMsg::Hello {
            user_id: "user-bob".into(),
            credential: "device-secret".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    );
}

pub(crate) fn send_welcome(stream: &mut impl Write, user_id: &str, name: &str) {
    send(
        stream,
        &ServerMsg::Welcome {
            user_id: user_id.into(),
            name: name.into(),
            client_id: "client-1".into(),
            tree: Tree::new(),
        },
    );
}

pub(crate) fn person(user_id: &str, name: &str, attached_clients: u32) -> Person {
    Person {
        online: true,
        user_id: user_id.into(),
        name: name.into(),
        attached_clients,
        peekable: true,
        host: false,
        state: PersonState::Idle,
        tabs: 2,
        foreground: "bash".into(),
        idle_secs: 90,
    }
}

pub(crate) fn send(stream: &mut impl Write, message: &ServerMsg) {
    codec::encode(stream, message).expect("server message must encode");
}

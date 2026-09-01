use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use seer_broker::{Config, UserConfig, serve};
use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[test]
fn handles_required_handshake_outcomes() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    let config = test_config(address);
    let _server = thread::spawn(move || serve(listener, &config));

    let (_, welcome) = exchange(
        address,
        &ClientMsg::Hello {
            user_id: "alice".into(),
            credential: "alice-secret".into(),
        },
    );
    assert_eq!(
        welcome,
        ServerMsg::Welcome {
            user_id: "alice".into(),
            name: "alice".into(),
            client_id: String::new(),
            tree: Tree::new(),
        }
    );

    let (bad_token_stream, bad_token) = exchange(
        address,
        &ClientMsg::Hello {
            user_id: "alice".into(),
            credential: "wrong".into(),
        },
    );
    assert_refused_and_closed(bad_token_stream, bad_token, "invalid credentials");

    let (non_hello_stream, non_hello) = exchange(address, &ClientMsg::Detach);
    assert_refused_and_closed(non_hello_stream, non_hello, "expected Hello");
}

#[test]
fn handles_a_second_connection_while_the_first_is_silent() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let address = listener
        .local_addr()
        .expect("listener must have an address");
    let config = test_config(address);
    let _server = thread::spawn(move || serve(listener, &config));
    let _silent = TcpStream::connect(address).expect("silent client must connect");

    let (stream, response) = exchange(
        address,
        &ClientMsg::Hello {
            user_id: "alice".into(),
            credential: "wrong".into(),
        },
    );

    assert_refused_and_closed(stream, response, "invalid credentials");
}

fn test_config(listen: SocketAddr) -> Config {
    Config {
        listen,
        shell: "sh".into(),
        users: vec![
            UserConfig {
                user: "alice".into(),
                token: "alice-secret".into(),
            },
            UserConfig {
                user: "bob".into(),
                token: "bob-secret".into(),
            },
        ],
    }
}

fn exchange(address: SocketAddr, request: &ClientMsg) -> (TcpStream, ServerMsg) {
    let mut stream = TcpStream::connect(address).expect("client must connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("read timeout must set");
    codec::encode(&mut stream, request).expect("request must encode");
    let response = codec::decode(&mut stream).expect("response must decode");
    (stream, response)
}

fn assert_refused_and_closed(mut stream: TcpStream, response: ServerMsg, reason: &str) {
    assert_eq!(
        response,
        ServerMsg::Refused {
            reason: reason.into()
        }
    );

    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("read timeout must set");
    let mut byte = [0];
    assert_eq!(stream.read(&mut byte).expect("connection must close"), 0);
}

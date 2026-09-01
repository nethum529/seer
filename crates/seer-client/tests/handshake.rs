use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[test]
fn handles_welcome_and_refused_replies() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let addr = listener
        .local_addr()
        .expect("listener must have an address");
    let server = thread::spawn(move || {
        answer(
            &listener,
            ClientMsg::Hello {
                user: "alice".into(),
                token: "valid".into(),
            },
            ServerMsg::Welcome {
                user: "alice".into(),
                tree: Tree::new(),
            },
        );
        answer(
            &listener,
            ClientMsg::Hello {
                user: "bob".into(),
                token: "invalid".into(),
            },
            ServerMsg::Refused {
                reason: "invalid token".into(),
            },
        );
    });

    let welcome = run_client(addr.to_string(), "alice", "valid");
    assert_eq!(welcome.status.code(), Some(0));
    assert_eq!(welcome.stdout, b"connected as alice\n");
    assert!(welcome.stderr.is_empty());

    let refused = run_client(addr.to_string(), "bob", "invalid");
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stdout.is_empty());
    assert_eq!(refused.stderr, b"refused: invalid token\n");

    server.join().expect("server must finish");
}

#[test]
fn sends_peek_after_welcome() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let addr = listener
        .local_addr()
        .expect("listener must have an address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("server must accept a client");
        assert_hello(
            &mut stream,
            ClientMsg::Hello {
                user: "bob".into(),
                token: "valid".into(),
            },
        );
        codec::encode(
            &mut stream,
            &ServerMsg::Welcome {
                user: "bob".into(),
                tree: Tree::new(),
            },
        )
        .expect("server must encode Welcome");
        let message: ClientMsg = codec::decode(&mut stream).expect("server must decode Peek");
        assert_eq!(
            message,
            ClientMsg::Peek {
                user: "alice".into(),
                workspace: "w1".into(),
            }
        );
    });

    let output = run(&[addr.to_string().as_str(), "bob", "valid", "--peek", "alice"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"connected as bob\n");
    assert!(output.stderr.is_empty());

    server.join().expect("server must finish");
}

#[test]
fn rejects_invalid_argument_counts() {
    let cases: &[&[&str]] = &[
        &[],
        &["addr"],
        &["addr", "user"],
        &["addr", "user", "token", "extra"],
        &["addr", "user", "token", "--peek"],
        &["addr", "user", "token", "--other", "alice"],
        &["addr", "user", "token", "--peek", "alice", "extra"],
    ];

    for arguments in cases {
        let output = run(arguments);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            output.stderr,
            b"usage: seer-client <addr> <user> <token> [--peek <target-user>]\n"
        );
    }
}

#[test]
fn rejects_unexpected_and_invalid_replies() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let addr = listener
        .local_addr()
        .expect("listener must have an address");
    let server = thread::spawn(move || {
        answer(
            &listener,
            ClientMsg::Hello {
                user: "alice".into(),
                token: "valid".into(),
            },
            ServerMsg::Tree { tree: Tree::new() },
        );

        let (mut stream, _) = listener.accept().expect("server must accept a client");
        assert_hello(
            &mut stream,
            ClientMsg::Hello {
                user: "alice".into(),
                token: "valid".into(),
            },
        );
        stream
            .write_all(&[0, 0, 0, 1, b'{'])
            .expect("server must write an invalid reply");
    });

    let unexpected = run_client(addr.to_string(), "alice", "valid");
    assert_eq!(unexpected.status.code(), Some(2));
    assert!(unexpected.stdout.is_empty());
    assert_eq!(unexpected.stderr, b"error: unexpected server reply\n");

    let invalid = run_client(addr.to_string(), "alice", "valid");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    let stderr = String::from_utf8(invalid.stderr).expect("error must be UTF-8");
    assert!(stderr.starts_with("error: "));
    assert_eq!(stderr.lines().count(), 1);

    server.join().expect("server must finish");
}

fn answer(listener: &TcpListener, expected: ClientMsg, reply: ServerMsg) {
    let (mut stream, _) = listener.accept().expect("server must accept a client");
    assert_hello(&mut stream, expected);
    codec::encode(&mut stream, &reply).expect("server must encode its reply");
}

fn assert_hello(stream: &mut TcpStream, expected: ClientMsg) {
    let hello: ClientMsg = codec::decode(stream).expect("server must decode Hello");
    assert_eq!(hello, expected);
}

fn run_client(addr: String, user: &str, token: &str) -> Output {
    run(&[addr.as_str(), user, token])
}

fn run(arguments: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_seer-client"))
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("client must start");
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        if child
            .try_wait()
            .expect("client status must be available")
            .is_some()
        {
            return child
                .wait_with_output()
                .expect("client output must be read");
        }
        if Instant::now() >= deadline {
            child.kill().expect("client must be killed after timeout");
            let _ = child.wait();
            panic!("client did not finish before timeout");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

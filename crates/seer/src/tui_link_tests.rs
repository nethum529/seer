use crate::state::ClientState;
use crate::store::ServerEntry;
use crate::tui_link::{Reconnects, event_channel};
use crate::version_skew::refusal_message;
use seer_core::Tree;
use seer_core::proto::{ClientMsg, ServerMsg, codec};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn answer_hello(listener: &TcpListener, reply: &ServerMsg) {
    let (mut stream, _) = listener.accept().expect("the window must connect");
    let hello: ClientMsg = codec::decode(&mut stream).expect("Hello must decode");
    assert!(matches!(hello, ClientMsg::Hello { .. }));
    codec::encode(&mut stream, reply).expect("reply must encode");
    if matches!(reply, ServerMsg::Welcome { .. }) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _: std::io::Result<ClientMsg> = codec::decode(&mut stream);
    }
}

#[test]
fn a_refused_window_shows_the_step_until_the_room_accepts() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener must bind");
    let port = listener.local_addr().expect("listener address").port();
    let reason = format!(
        "version mismatch: server 0.1.0, client {}. Run: seer update",
        env!("CARGO_PKG_VERSION")
    );
    let (accept_now, accept_signal) = mpsc::channel::<()>();
    let refusal = reason.clone();
    let room = thread::spawn(move || {
        answer_hello(&listener, &ServerMsg::Refused { reason: refusal });
        accept_signal.recv().expect("the test must signal");
        answer_hello(
            &listener,
            &ServerMsg::Welcome {
                user_id: "user-bob".into(),
                name: "bob".into(),
                client_id: "client-1".into(),
                tree: Tree::new(),
            },
        );
    });
    let server = ServerEntry {
        endpoint: format!("127.0.0.1:{port}"),
        alias: "team.example.com".into(),
        user_id: "user-bob".into(),
        name: "bob".into(),
        credential: "secret".into(),
        current: true,
    };
    let (_events, sender) = event_channel();
    let reconnects = Reconnects::new(server, sender);
    let mut state = ClientState::new(Tree::new(), "user-bob".into());
    reconnects.start();

    let expected = format!("team.example.com: {}", refusal_message(&reason, false));
    let deadline = Instant::now() + Duration::from_secs(10);
    while state.room_notice.is_none() && Instant::now() < deadline {
        assert!(reconnects.take(&mut state).is_none());
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(state.room_notice.as_deref(), Some(expected.as_str()));
    assert!(expected.contains("The room owner must update."));

    accept_now.send(()).expect("the room thread must wait");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut accepted = None;
    while accepted.is_none() && Instant::now() < deadline {
        accepted = reconnects.take(&mut state);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(accepted.is_some(), "the room must accept the window");
    assert_eq!(state.room_notice, None);
    reconnects.stop();
    drop(accepted);
    room.join().expect("the room thread must finish");
}

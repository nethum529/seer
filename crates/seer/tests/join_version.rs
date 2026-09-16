use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use seer_core::proto::{ClientMsg, ServerMsg, codec};

#[path = "support/cli_run.rs"]
mod cli_run;

use cli_run::{TestConfig, run, text};

#[test]
fn join_stops_before_it_uses_the_seat_of_a_room_of_another_minor_version() {
    let config = TestConfig::new();
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake room must bind");
    let port = listener.local_addr().expect("fake room address").port();
    let seat_used = Arc::new(AtomicBool::new(false));
    let room_seat = Arc::clone(&seat_used);
    thread::spawn(move || old_room(&listener, &room_seat));

    let invitation = format!("SEER1-127.0.0.1-{port}-seat-token");
    let joined = run(&config, &["join", &invitation], "");

    assert!(!joined.status.success());
    assert!(!text(&joined.stdout).contains("Name"));
    assert!(
        text(&joined.stderr).contains(&format!(
            "You run Seer {}. The room owner must update.",
            env!("CARGO_PKG_VERSION")
        )),
        "{}",
        text(&joined.stderr)
    );
    assert!(!config.root.join("seer/servers.toml").exists());
    assert!(
        !seat_used.load(Ordering::SeqCst),
        "the seat must stay unused"
    );
}

// A room server from before the Join version check, one minor version older.
fn old_room(listener: &TcpListener, seat_used: &AtomicBool) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            return;
        };
        let reply = match codec::decode(&mut stream) {
            Ok(ClientMsg::Hello { version, .. }) => ServerMsg::Refused {
                reason: format!(
                    "version mismatch: server 0.5.0, client {version}. Run: seer update"
                ),
            },
            Ok(ClientMsg::Join { name, .. }) => {
                seat_used.store(true, Ordering::SeqCst);
                ServerMsg::Joined {
                    user_id: "u-alice".into(),
                    credential: "secret".into(),
                    name,
                }
            }
            _ => continue,
        };
        let _ = codec::encode(&mut stream, &reply);
    }
}

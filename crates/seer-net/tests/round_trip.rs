use seer_net::{Listener, dial, load_or_create_secret_key};
use std::fs;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TIMEOUT: Duration = Duration::from_secs(60);

#[test]
fn round_trip_between_two_endpoints() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after the Unix epoch")
        .as_nanos();
    let key_dir = std::env::temp_dir().join(format!("seer-net-{}-{unique}", std::process::id()));
    fs::create_dir(&key_dir).expect("temporary key directory must be created");

    let listener_key = load_or_create_secret_key(&key_dir.join("listener.key"))
        .expect("listener key must load or be created");
    let dialer_key = load_or_create_secret_key(&key_dir.join("dialer.key"))
        .expect("dialer key must load or be created");
    let (bind_result_tx, bind_result_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = bind_result_tx.send(Listener::bind(listener_key));
    });
    let listener = bind_result_rx
        .recv_timeout(TIMEOUT)
        .expect("listener bind timed out; check network access")
        .expect("listener must bind");
    let listener_id = listener.id();

    let (server_result_tx, server_result_rx) = mpsc::channel();
    thread::spawn(move || {
        let result = listener
            .accept()
            .and_then(|(_remote, mut stream, _session)| {
                stream.set_read_timeout(Some(TIMEOUT))?;
                stream.set_write_timeout(Some(TIMEOUT))?;
                let mut request = [0_u8; 5];
                stream.read_exact(&mut request)?;
                if request != *b"ping\n" {
                    return Err(std::io::Error::other("server received an invalid request"));
                }
                stream.write_all(b"pong\n")?;
                stream.flush()
            });
        let _ = server_result_tx.send(result);
    });

    let (dial_result_tx, dial_result_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = dial_result_tx.send(dial(dialer_key, listener_id));
    });
    let mut client = dial_result_rx
        .recv_timeout(TIMEOUT)
        .expect("dial timed out; check network access")
        .expect("dial must connect to the listener");
    client
        .set_read_timeout(Some(TIMEOUT))
        .expect("client read timeout must be set");
    client
        .set_write_timeout(Some(TIMEOUT))
        .expect("client write timeout must be set");
    client.write_all(b"ping\n").expect("client must send ping");
    client.flush().expect("client must flush ping");

    let mut response = [0_u8; 5];
    client
        .read_exact(&mut response)
        .expect("client must receive pong before the timeout");
    assert_eq!(&response, b"pong\n");
    server_result_rx
        .recv_timeout(TIMEOUT)
        .expect("server exchange timed out; check network access")
        .expect("server exchange must succeed");

    fs::remove_dir_all(key_dir).expect("temporary key directory must be removed");
}

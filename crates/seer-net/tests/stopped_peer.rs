use seer_net::{EndpointId, Listener, SecretKey, decode_endpoint_id, dial, encode_endpoint_id};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

const CHILD_ENV: &str = "SEER_NET_STOPPED_PEER_CHILD";
const ID_MARKER: &str = "listener-id:";
const SETUP_TIMEOUT: Duration = Duration::from_secs(60);
const GIVE_UP_BOUND: Duration = Duration::from_secs(10);

#[test]
fn listener_process() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let listener = Listener::bind(SecretKey::generate()).expect("listener must bind");
    println!("{ID_MARKER} {}", encode_endpoint_id(listener.id()));
    let _ = io::stdin().read_to_end(&mut Vec::new());
}

// Stopped state: the listener process published its address and was then
// killed, as in the peerdown run of docs/research/20-address-lookup.md. All
// dials use one key, as the seer binary does with its device key.
#[test]
fn dial_gives_up_on_a_killed_listener_process() {
    let mut child = Command::new(std::env::current_exe().expect("test binary path must be known"))
        .args(["--exact", "listener_process", "--nocapture"])
        .env(CHILD_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("listener process must start");
    let stdout = child.stdout.take().expect("listener stdout must be piped");
    let (id_tx, id_rx) = mpsc::channel();
    thread::spawn(move || {
        let id = BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .find_map(|line| {
                let (_, rest) = line.split_once(ID_MARKER)?;
                rest.split_whitespace().next().map(str::to_owned)
            });
        let _ = id_tx.send(id);
    });
    let live = Listener::bind(SecretKey::generate()).expect("live listener must bind");
    let live_id = live.id();
    thread::spawn(move || serve_echo(&live));
    let id = id_rx
        .recv_timeout(SETUP_TIMEOUT)
        .expect("listener process timed out; check network access")
        .expect("listener process must print its endpoint ID");
    let id = decode_endpoint_id(&id).expect("listener endpoint ID must parse");
    let key = SecretKey::generate();

    let mut other = dial_within(&key, live_id, SETUP_TIMEOUT)
        .expect("live dial timed out; check network access")
        .expect("dial must reach the live listener");
    echo(&mut other).expect("live connection must echo");
    let first = dial_within(&key, id, SETUP_TIMEOUT)
        .expect("first dial timed out; check network access")
        .expect("first dial must reach the running listener");
    child.kill().expect("listener process must be killed");
    child.wait().expect("listener process must exit");

    match dial_within(&key, id, GIVE_UP_BOUND) {
        Ok(result) => assert!(result.is_err(), "dial to a killed listener must fail"),
        Err(_) => panic!("dial to a killed listener did not give up within 10 s"),
    }
    echo(&mut other).expect("live connection must still echo after the failed dial");
    drop(first);
    let mut later = dial_within(&key, live_id, SETUP_TIMEOUT)
        .expect("later dial timed out")
        .expect("later dial must reach the live listener");
    echo(&mut later).expect("later connection must echo");
}

fn serve_echo(listener: &Listener) {
    while let Ok((_, stream, _)) = listener.accept() {
        thread::spawn(move || io::copy(&mut &stream, &mut &stream));
    }
}

fn echo(stream: &mut UnixStream) -> io::Result<()> {
    stream.set_read_timeout(Some(GIVE_UP_BOUND))?;
    stream.write_all(b"ping")?;
    let mut back = [0_u8; 4];
    stream.read_exact(&mut back)?;
    assert_eq!(&back, b"ping");
    Ok(())
}

fn dial_within(
    key: &SecretKey,
    remote: EndpointId,
    bound: Duration,
) -> Result<io::Result<UnixStream>, RecvTimeoutError> {
    let key = key.clone();
    let (result_tx, result_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = result_tx.send(dial(key, remote));
    });
    result_rx.recv_timeout(bound)
}

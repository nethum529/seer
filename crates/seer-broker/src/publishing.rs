use std::io;
use std::io::Read;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::{ClientMsg, ServerMsg, codec};
use seer_net::{Session, Socket, Stream};

use crate::server::{
    BrokerState, Handshake, INVALID_CREDENTIALS, authenticate, read_message, refuse,
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn publish_handshake(
    stream: &mut Socket,
    broker: &BrokerState,
    user_id: String,
    credential: &str,
    generation: String,
) -> io::Result<Handshake> {
    if authenticate(stream, broker.registry(), &user_id, credential)?.is_none() {
        return Ok(Handshake::Done);
    }
    Ok(Handshake::Runtime {
        user_id,
        generation,
    })
}

// The broker never starts or stops the process behind a registration.
pub(crate) fn serve_runtime(
    mut stream: Socket,
    broker: &Arc<BrokerState>,
    user_id: &str,
    generation: &str,
    session: Option<Session>,
) -> io::Result<()> {
    let registration = match broker
        .runtimes()
        .publish(user_id, generation, stream.clone())
    {
        Ok(registration) => registration,
        Err(error) => return refuse(&mut stream, &error.to_string()),
    };
    // Every path after the registration must retire it. A runtime that is
    // left published locks that person out of the room for good.
    let result = confirm_and_hold(&mut stream, broker, generation, session);
    broker.runtimes().retire(user_id, &registration);
    let _ = broker.publish_people();
    result
}

fn confirm_and_hold(
    stream: &mut Socket,
    broker: &Arc<BrokerState>,
    generation: &str,
    session: Option<Session>,
) -> io::Result<()> {
    codec::encode(
        stream,
        &ServerMsg::Published {
            generation: generation.to_owned(),
        },
    )?;
    if let Some(session) = session {
        spawn_stream_pump(session, Arc::clone(broker));
    }
    let _ = broker.publish_people();
    wait_for_close(stream)
}

fn wait_for_close(stream: &mut Socket) -> io::Result<()> {
    stream.set_read_timeout(None)?;
    let mut byte = [0_u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Ok(()),
        }
    }
}

fn spawn_stream_pump(session: Session, broker: Arc<BrokerState>) {
    thread::spawn(move || {
        while let Ok(stream) = session.accept() {
            let broker = Arc::clone(&broker);
            thread::spawn(move || {
                let mut stream = Socket::from(stream);
                if let Err(error) = adopt_runtime_stream(&mut stream, &broker) {
                    eprintln!("broker dropped a runtime stream: {error}");
                }
            });
        }
    });
}

fn adopt_runtime_stream(stream: &mut Socket, broker: &BrokerState) -> io::Result<()> {
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let message = read_message(stream, deadline)?;
    let ClientMsg::RuntimeStream {
        user_id,
        credential,
        token,
    } = message
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a runtime stream",
        ));
    };
    claim_runtime_stream(stream, broker, &user_id, &credential, &token)
}

pub(crate) fn claim_runtime_stream(
    stream: &mut Socket,
    broker: &BrokerState,
    user_id: &str,
    credential: &str,
    token: &str,
) -> io::Result<()> {
    if broker
        .registry()
        .authenticate(user_id, credential)?
        .is_none()
    {
        return refuse(stream, INVALID_CREDENTIALS);
    }
    stream.set_read_timeout(None)?;
    if broker.runtimes().deliver(user_id, token, stream.clone()) {
        return Ok(());
    }
    Err(io::Error::other("no viewer waited for this runtime stream"))
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;

    use seer_net::Socket;

    use crate::server::BrokerState;
    use crate::test_support::{remove_directory, temporary_directory};

    #[test]
    fn a_runtime_that_never_confirmed_does_not_stay_published() {
        let directory = temporary_directory("broker-publish");
        let config = crate::Config {
            listen: "127.0.0.1:0".parse().expect("address must parse"),
            published_addr: "host:7321".into(),
            remote: false,
            state_dir: directory.clone(),
            owner_name: "owner".into(),
        };
        let (state, owner) = BrokerState::new(&config).expect("broker state must open");
        let (user_id, _) = owner.expect("the owner identity must be minted");
        let broker = Arc::new(state);
        let (near, far) = UnixStream::pair().expect("streams must open");
        drop(far);

        let _ = super::serve_runtime(Socket::from(near), &broker, &user_id, "gen-1", None);

        assert!(
            !broker.runtimes().is_running(&user_id),
            "a runtime that failed before confirming must not lock the person out"
        );
        remove_directory(&directory, "broker state must be removed");
    }
}

use super::*;
use std::fs;
use std::io;
use std::net::{Shutdown, TcpStream};
use std::thread;

#[test]
fn revocation_during_setup_blocks_typed_input() {
    revoked_during_setup(|pane| ClientMsg::InputInto {
        user: "alice".into(),
        pane,
        input: seer_core::TerminalInput::new(seer_core::InputEvent::Text("typed".into())),
    });
}

#[test]
fn revocation_during_setup_blocks_byte_input() {
    revoked_during_setup(|pane| ClientMsg::TypeInto {
        user: "alice".into(),
        pane,
        bytes: b"bytes".to_vec(),
    });
}

#[test]
fn revocation_during_setup_blocks_mouse_input() {
    revoked_during_setup(|pane| ClientMsg::MouseInto {
        user: "alice".into(),
        pane,
        mouse: seer_core::MouseInput {
            kind: seer_core::MouseKind::Down,
            button: Some(seer_core::MouseButton::Left),
            column: 5,
            row: 2,
            modifiers: seer_core::Modifiers::default(),
        },
    });
}

fn revoked_during_setup(message: impl FnOnce(String) -> ClientMsg) {
    let mut room = Room::start(false);
    let mut window = room.publish_to("127.0.0.1:0", "alice", ALICE_SECRET);
    let at = first_terminal(&own_tree(&mut window));
    let capture = room.root.join("input");
    let script = room.root.join("capture.py");
    fs::write(
        &script,
        format!(
            "import os,tty\ntty.setraw(0)\nf=open('{}','ab',buffering=0)\nos.write(1,b'\\x1b[?1000;1006hReady451')\nwhile True:\n f.write(os.read(0,4096))\n",
            capture.display()
        ),
    )
    .unwrap();
    type_locally(
        &mut window,
        &at,
        &format!("exec python3 -u '{}'\n", script.display()),
    );
    loop {
        if let ServerMsg::Cells { frame, .. } = decode(&mut window)
            && frame_text(&frame).contains("Ready451")
        {
            break;
        }
    }
    let mut publisher = TcpStream::connect(room.address).unwrap();
    publisher.set_read_timeout(Some(WAIT)).unwrap();
    send(
        &mut publisher,
        &ClientMsg::PublishRuntime {
            user_id: "alice".into(),
            credential: ALICE_SECRET.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            generation: "delayed".into(),
        },
    );
    wait_for(&mut publisher, |m| matches!(m, ServerMsg::Published { .. }));
    let mut alice = join_room(room.address, "alice", ALICE_SECRET);
    let mut bob = join_room(room.address, "bob", BOB_SECRET);
    send(
        &mut alice,
        &ClientMsg::SetGrant {
            user: "bob".into(),
            can_type: true,
        },
    );
    wait_for_grant(&mut bob, true);
    send(&mut bob, &message(at.pane));
    let remote = pending_viewer(&mut publisher, room.address);
    send(
        &mut alice,
        &ClientMsg::SetGrant {
            user: "bob".into(),
            can_type: false,
        },
    );
    wait_for(
        &mut alice,
        |m| matches!(m, ServerMsg::Grants { can_type_here, .. } if can_type_here.is_empty()),
    );
    wait_for_grant(&mut bob, false);
    send(
        &mut bob,
        &ClientMsg::DetachClient {
            client_id: String::new(),
        },
    );
    let mut local = UnixStream::connect(room.root.join("alice/socket")).unwrap();
    let ready: ServerMsg = codec::decode(&mut local).unwrap();
    send(&mut local, &ClientMsg::ObserveRuntime);
    let mut reply = remote.try_clone().unwrap();
    codec::encode(&mut reply, &ready).unwrap();
    let relay = relay(remote, local);
    let mut refused = false;
    loop {
        match decode(&mut bob) {
            ServerMsg::Refused { .. } => refused = true,
            ServerMsg::Clients { .. } => break,
            _ => {}
        }
    }
    relay.shutdown(Shutdown::Both).unwrap();
    assert!(refused, "revoked input must be refused after runtime setup");
    assert!(fs::read(&capture).unwrap().is_empty());
}

fn pending_viewer(publisher: &mut TcpStream, address: std::net::SocketAddr) -> TcpStream {
    loop {
        let ServerMsg::OpenStream { token } = decode(publisher) else {
            continue;
        };
        let mut stream = TcpStream::connect(address).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        send(
            &mut stream,
            &ClientMsg::RuntimeStream {
                user_id: "alice".into(),
                credential: ALICE_SECRET.into(),
                token,
            },
        );
        if matches!(
            codec::decode::<_, ClientMsg>(&mut stream).unwrap(),
            ClientMsg::ObserveRuntime
        ) {
            stream.set_read_timeout(None).unwrap();
            return stream;
        }
    }
}

fn relay(remote: TcpStream, local: UnixStream) -> TcpStream {
    let stop = remote.try_clone().unwrap();
    let mut remote_reader = remote.try_clone().unwrap();
    let mut local_writer = local.try_clone().unwrap();
    thread::spawn(move || {
        let _ = io::copy(&mut remote_reader, &mut local_writer);
        let _ = local_writer.shutdown(Shutdown::Both);
    });
    thread::spawn(move || {
        let mut local = local;
        let mut remote = remote;
        let _ = io::copy(&mut local, &mut remote);
        let _ = remote.shutdown(Shutdown::Both);
    });
    stop
}

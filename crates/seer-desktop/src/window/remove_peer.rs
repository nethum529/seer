use seer_core::{
    Tree,
    proto::{ClientMsg, ServerMsg, codec},
};
use std::{
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

pub(super) struct Room {
    pub(super) server: seer::ServerEntry,
    state: Arc<State>,
    worker: Option<thread::JoinHandle<()>>,
}
struct State {
    host: bool,
    done: AtomicBool,
    fail: AtomicBool,
    removed: AtomicBool,
    received: Mutex<Vec<ClientMsg>>,
    sockets: Mutex<Vec<TcpStream>>,
}
impl Room {
    pub(super) fn start(host: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = serde_json::from_value(serde_json::json!({"endpoint":listener.local_addr().unwrap().to_string(),"alias":"E2e","user_id":"e2e","name":if host {"Host"} else {"Guest"},"credential":"e2e","current":true})).unwrap();
        let state = Arc::new(State {
            host,
            done: AtomicBool::new(false),
            fail: AtomicBool::new(true),
            removed: AtomicBool::new(false),
            received: Mutex::default(),
            sockets: Mutex::default(),
        });
        let peer = state.clone();
        let worker = thread::spawn(move || {
            let mut workers = Vec::new();
            while !peer.done.load(Ordering::Acquire) {
                if let Ok((socket, _)) = listener.accept() {
                    let state = peer.clone();
                    workers.push(thread::spawn(move || serve(socket, &state)));
                } else {
                    thread::sleep(Duration::from_millis(5));
                }
            }
            for socket in peer.sockets.lock().unwrap().iter() {
                let _ = socket.shutdown(Shutdown::Both);
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            server,
            state,
            worker: Some(worker),
        }
    }
    pub(super) fn connections(&self) -> usize {
        self.state.sockets.lock().unwrap().len()
    }
    pub(super) fn received(&self) -> Vec<ClientMsg> {
        self.state.received.lock().unwrap().clone()
    }
    pub(super) fn removals(&self) -> usize {
        self.received()
            .iter()
            .filter(|m| matches!(m, ClientMsg::RemovePerson { .. }))
            .count()
    }
    pub(super) fn retry(&self) {
        self.state.fail.store(false, Ordering::Release);
    }
    pub(super) fn remove_guest(&self) {
        broadcast(
            &self.state,
            &ServerMsg::Bye {
                reason: "removed by host".into(),
            },
        );
    }
}
impl Drop for Room {
    fn drop(&mut self) {
        self.state.done.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn people(state: &State) -> ServerMsg {
    let own = if state.host { "Host" } else { "Guest" };
    let other = if state.host { "Guest" } else { "Host" };
    ServerMsg::People {people: [("e2e",own,state.host),("other",other,!state.host)].into_iter().filter(|(id,_,_)| *id == "e2e" || !state.removed.load(Ordering::Acquire)).map(|(id,name,host)|serde_json::from_value(serde_json::json!({"user_id":id,"name":name,"host":host,"online":true,"peekable":true,"attached_clients":2})).unwrap()).collect()}
}
fn broadcast(state: &State, message: &ServerMsg) {
    for mut socket in state.sockets.lock().unwrap().iter() {
        let _ = codec::encode(&mut socket, message);
    }
}
fn serve(mut socket: TcpStream, state: &State) {
    socket.set_nonblocking(false).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    if !matches!(codec::decode(&mut socket), Ok(ClientMsg::Hello { .. })) {
        return;
    }
    let welcome = ServerMsg::Welcome {
        user_id: "e2e".into(),
        name: if state.host { "Host" } else { "Guest" }.into(),
        client_id: "remove".into(),
        tree: Tree::new(),
    };
    if codec::encode(&mut socket, &welcome).is_err() {
        return;
    }
    state
        .sockets
        .lock()
        .unwrap()
        .push(socket.try_clone().unwrap());
    socket.set_read_timeout(None).unwrap();
    while let Ok(message) = codec::decode::<_, ClientMsg>(&mut socket) {
        state.received.lock().unwrap().push(message.clone());
        let reply = match message {
            ClientMsg::ListPeople => people(state),
            ClientMsg::RemovePerson { name } => {
                if state.fail.load(Ordering::Acquire) {
                    ServerMsg::Refused {
                        reason: "Could not remove Guest. Try again.".into(),
                    }
                } else {
                    state.removed.store(true, Ordering::Release);
                    broadcast(state, &people(state));
                    ServerMsg::PersonRemoved { name }
                }
            }
            _ => continue,
        };
        if codec::encode(&mut socket, &reply).is_err() {
            break;
        }
    }
}

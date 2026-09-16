// Issue 421: a runtime from before 0.6.0 greets a window without a version.

use std::io::Write;
use std::os::unix::net::UnixListener;

use super::*;

#[test]
fn a_window_on_a_runtime_without_a_version_shows_the_restart_notice() {
    let directory = env::temp_dir().join(format!("seer-old-runtime-{}", std::process::id()));
    fs::create_dir_all(&directory).expect("test directory must exist");
    let socket = directory.join("socket");
    let _ = fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).expect("fake runtime must listen");
    let runtime = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("window must connect");
        let greeting = br#"{"RuntimeReady":{"generation":"old"}}"#;
        let length = u32::try_from(greeting.len()).expect("frame length must fit in u32");
        stream
            .write_all(&length.to_be_bytes())
            .and_then(|()| stream.write_all(greeting))
            .expect("greeting must send");
        let _: ClientMsg = codec::decode(&mut stream).expect("attach must decode");
        codec::encode(&mut stream, &ServerMsg::Tree { tree: Tree::new() }).expect("tree must send");
    });

    let ready = greet(&socket, None, READY_TIMEOUT).expect("the old greeting must decode");
    let server = ServerEntry {
        endpoint: "127.0.0.1:9".into(),
        alias: "room".into(),
        user_id: "owner".into(),
        name: "owner".into(),
        credential: "credential".into(),
        current: true,
    };
    let (_, _, notice) = attach_stream(&server, ready).expect("the window must attach");
    runtime.join().expect("fake runtime must stop");
    let _ = fs::remove_dir_all(&directory);

    let notice = notice.expect("an old runtime must give a standing notice");
    assert!(notice.contains("seer restart"), "{notice}");
    assert!(notice.contains("ends the running shells"), "{notice}");
    assert_eq!(runtime_notice(Some(env!("CARGO_PKG_VERSION"))), None);
}

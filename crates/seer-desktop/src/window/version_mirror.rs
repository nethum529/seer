use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub(super) fn prepare(root: PathBuf) -> (String, impl FnOnce()) {
    let source = root.join("release");
    fs::create_dir(&source).unwrap();
    for name in ["seer", "seer-broker", "seer-runtime", "version"] {
        fs::write(source.join(name), "fixture\n").unwrap();
    }
    let marker = root.join("installed");
    fs::write(
        source.join("install.sh"),
        format!("#!/bin/sh\nprintf done > '{}'\n", marker.display()),
    )
    .unwrap();
    let archive = root.join("release.tar.gz");
    assert!(
        Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&source)
            .arg(".")
            .status()
            .unwrap()
            .success()
    );
    let bytes = fs::read(archive).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    (format!("http://{address}/latest"), move || {
        thread::spawn(move || {
            let mut attempts = 0;
            for connection in listener.incoming() {
                let mut socket = connection.unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut line = String::new();
                BufReader::new(&mut socket).read_line(&mut line).unwrap();
                let (status, extra, body) = if line.starts_with("GET /latest/") {
                    attempts += 1;
                    fs::write(root.join(format!("request-{attempts}")), []).unwrap();
                    let deadline = Instant::now() + Duration::from_secs(30);
                    while !root.join(format!("release-{attempts}")).exists() {
                        assert!(Instant::now() < deadline, "update fixture was not released");
                        thread::sleep(Duration::from_millis(10));
                    }
                    if attempts == 1 {
                        ("503 Unavailable", String::new(), &[][..])
                    } else {
                        (
                            "302 Found",
                            format!(
                                "Location: http://{address}/download/v99.0.0/release.tar.gz\r\n"
                            ),
                            &[][..],
                        )
                    }
                } else {
                    ("200 OK", String::new(), bytes.as_slice())
                };
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                socket.write_all(body).unwrap();
            }
        });
    })
}

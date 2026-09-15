use crate::counters::counters;
use crate::{Args, Report, STEP_TIMEOUT, target};
use iroh::SecretKey;
use seer_net::{Listener, dial, dial_session};
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;

pub(crate) fn seer_listen(args: &Args, report: &Report) {
    for sample in 0..args.samples {
        report.step_blocking(sample, "seer_listen_ready", || {
            Listener::bind(SecretKey::generate())
        });
    }
}

pub(crate) fn seer_serve() -> io::Result<()> {
    let listener = Listener::bind(SecretKey::generate())?;
    println!("ready id={} relay=- loopback=- ips=-", listener.id());
    loop {
        let (_remote, stream, session) = listener.accept()?;
        thread::spawn(move || echo_stream(stream));
        thread::spawn(move || {
            while let Ok(stream) = session.accept() {
                thread::spawn(move || echo_stream(stream));
            }
        });
    }
}

fn echo_stream(mut stream: UnixStream) {
    let mut buffer = [0_u8; 4096];
    while let Ok(read) = stream.read(&mut buffer) {
        if read == 0 || stream.write_all(&buffer[..read]).is_err() {
            return;
        }
    }
}

// Warm samples call dial again in the same process with the same key, as a
// runtime does when it reconnects.
pub(crate) fn seer_dial(args: &Args, report: &Report) -> io::Result<()> {
    let remote = target(args)?.id;
    let key = SecretKey::generate();
    for sample in 0..args.samples {
        let dialed = report.step_blocking(sample, "seer_dial", || dial(key.clone(), remote));
        if let Some(stream) = dialed {
            let first = report.step_blocking(sample, "stream_open", || first_echo(stream));
            unix_echoes(report, sample, first, args.echoes);
        }
    }
    Ok(())
}

// Sample 0 pays for the session. Warm samples open a new stream on it.
pub(crate) fn seer_session(args: &Args, report: &Report) -> io::Result<()> {
    let remote = target(args)?.id;
    let session = report.step_blocking(0, "seer_dial_session", || {
        dial_session(SecretKey::generate(), remote)
    });
    let Some(session) = session else {
        return Ok(());
    };
    for sample in 0..args.samples {
        let first = report.step_blocking(sample, "stream_open", || first_echo(session.open()?));
        unix_echoes(report, sample, first, args.echoes);
    }
    Ok(())
}

fn first_echo(mut stream: UnixStream) -> io::Result<UnixStream> {
    stream.set_read_timeout(Some(STEP_TIMEOUT))?;
    unix_echo(&mut stream, b"x")?;
    Ok(stream)
}

fn unix_echoes(report: &Report, sample: usize, stream: Option<UnixStream>, echoes: usize) {
    let Some(mut stream) = stream else {
        return;
    };
    let before = counters();
    for _ in 0..echoes {
        if report
            .step_blocking(sample, "echo_rtt", || {
                unix_echo(&mut stream, &report.payload)
            })
            .is_none()
        {
            return;
        }
    }
    report.counters(sample, echoes, &before);
}

// In both apis the dialer writes all bytes, then reads them back.
fn unix_echo(stream: &mut UnixStream, data: &[u8]) -> io::Result<()> {
    stream.write_all(data)?;
    let mut back = vec![0_u8; data.len()];
    stream.read_exact(&mut back)
}

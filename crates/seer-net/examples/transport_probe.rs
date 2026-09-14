// Measurement probe for issue 389. The method and the boundaries are in
// docs/research/18-transport-baseline.md. scripts/perf/transport.sh drives it.

use iroh::endpoint::{Connection, Incoming, RecvStream, SendStream, presets};
use iroh::{Endpoint, EndpointAddr, RelayUrl, SecretKey};
use seer_net::{ALPN, Listener, decode_endpoint_id, dial, dial_session};
use std::env;
use std::fmt::Display;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::process::ExitCode;
use std::str::FromStr;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use tokio::time::timeout;

const STEP_TIMEOUT: Duration = Duration::from_secs(15);
const USAGE: &str = "usage: transport_probe serve|listen|dial|session --api iroh|seer \
[--condition default|loopback|lan|relay|discovery] [--samples N] [--echoes N] \
[--remote ID] [--relay URL] [--ip ADDR]";

struct Args {
    command: String,
    api: String,
    condition: String,
    samples: usize,
    echoes: usize,
    remote: Option<String>,
    relay: Option<String>,
    ip: Option<String>,
}

impl Args {
    fn relay_only(&self) -> bool {
        self.condition == "relay"
    }
}

struct Report {
    workload: String,
    condition: String,
}

impl Report {
    fn line(&self, sample: usize, boundary: &str, ms: f64, status: &str) {
        let cache = if sample == 0 { "cold" } else { "warm" };
        println!(
            "{},{},{cache},{sample},{boundary},{ms:.3},{status}",
            self.workload, self.condition
        );
    }

    fn result<T, E: Display>(
        &self,
        sample: usize,
        boundary: &str,
        started: Instant,
        result: Result<T, E>,
    ) -> Option<T> {
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        match result {
            Ok(value) => {
                self.line(sample, boundary, ms, "ok");
                Some(value)
            }
            Err(error) => {
                let reason: String = error
                    .to_string()
                    .chars()
                    .map(|c| if c == ',' || c.is_control() { ' ' } else { c })
                    .collect();
                self.line(sample, boundary, ms, &format!("fail:{reason}"));
                None
            }
        }
    }

    async fn step<T, E: Display>(
        &self,
        sample: usize,
        boundary: &str,
        future: impl Future<Output = Result<T, E>>,
    ) -> Option<T> {
        let started = Instant::now();
        let result = match timeout(STEP_TIMEOUT, future).await {
            Ok(result) => result.map_err(|error| error.to_string()),
            Err(_) => Err("timeout".to_owned()),
        };
        self.result(sample, boundary, started, result)
    }

    fn step_blocking<T, E: Display>(
        &self,
        sample: usize,
        boundary: &str,
        work: impl FnOnce() -> Result<T, E>,
    ) -> Option<T> {
        let started = Instant::now();
        self.result(sample, boundary, started, work())
    }
}

fn main() -> ExitCode {
    let entered = SystemTime::now();
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let report = Report {
        workload: format!("{}_{}", args.api, args.command),
        condition: args.condition.clone(),
    };
    report_spawn(&report, entered);
    match run(&args, &report) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("transport_probe: {error}");
            ExitCode::from(2)
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut raw = env::args().skip(1);
    let mut args = Args {
        command: raw.next().ok_or(USAGE)?,
        api: "iroh".to_owned(),
        condition: "default".to_owned(),
        samples: 1,
        echoes: 20,
        remote: None,
        relay: None,
        ip: None,
    };
    while let Some(flag) = raw.next() {
        let value = raw.next().ok_or(USAGE)?;
        match flag.as_str() {
            "--api" => args.api = value,
            "--condition" => args.condition = value,
            "--samples" => args.samples = value.parse().map_err(|_| USAGE)?,
            "--echoes" => args.echoes = value.parse().map_err(|_| USAGE)?,
            "--remote" => args.remote = Some(value),
            "--relay" => args.relay = Some(value),
            "--ip" => args.ip = Some(value),
            _ => return Err(USAGE.to_owned()),
        }
    }
    Ok(args)
}

// The driver script writes the wall clock in microseconds into this variable
// just before it starts the process.
fn report_spawn(report: &Report, entered: SystemTime) {
    let Some(spawned) = env::var("SEER_PROBE_SPAWN_US")
        .ok()
        .and_then(|value| value.parse::<u128>().ok())
    else {
        return;
    };
    let Ok(entered) = entered.duration_since(UNIX_EPOCH) else {
        return;
    };
    let micros = entered.as_micros().saturating_sub(spawned);
    report.line(0, "process_spawn", micros as f64 / 1000.0, "ok");
}

fn run(args: &Args, report: &Report) -> io::Result<()> {
    match (args.command.as_str(), args.api.as_str()) {
        ("serve", "seer") => seer_serve(),
        ("listen", "seer") => {
            seer_listen(args, report);
            Ok(())
        }
        ("dial", "seer") => seer_dial(args, report),
        ("session", "seer") => seer_session(args, report),
        ("serve" | "listen" | "dial", "iroh") => {
            let runtime = report
                .step_blocking(0, "runtime_start", build_runtime)
                .ok_or_else(|| io::Error::other("could not start runtime"))?;
            runtime.block_on(run_iroh(args, report))
        }
        _ => Err(io::Error::other(USAGE)),
    }
}

// Same runtime shape as build_runtime in seer-net.
fn build_runtime() -> io::Result<Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

async fn run_iroh(args: &Args, report: &Report) -> io::Result<()> {
    match args.command.as_str() {
        "serve" => iroh_serve(args).await,
        "listen" => {
            iroh_listen(args, report).await;
            Ok(())
        }
        _ => iroh_dial(args, report).await,
    }
}

// Same builder as bind_endpoint in seer-net. The relay condition removes the
// IP transports so that every packet goes through the relay.
async fn bind(relay_only: bool) -> Result<Endpoint, iroh::endpoint::BindError> {
    let mut builder = Endpoint::builder(presets::N0)
        .secret_key(SecretKey::generate())
        .alpns(vec![ALPN.to_vec()]);
    if relay_only {
        builder = builder.clear_ip_transports();
    }
    builder.bind().await
}

async fn online(endpoint: &Endpoint) -> Result<(), std::convert::Infallible> {
    endpoint.online().await;
    Ok(())
}

async fn iroh_listen(args: &Args, report: &Report) {
    for sample in 0..args.samples {
        let Some(endpoint) = report
            .step(sample, "endpoint_bind", bind(args.relay_only()))
            .await
        else {
            continue;
        };
        report.step(sample, "relay_online", online(&endpoint)).await;
        endpoint.close().await;
    }
}

async fn iroh_serve(args: &Args) -> io::Result<()> {
    let endpoint = bind(args.relay_only()).await.map_err(io::Error::other)?;
    endpoint.online().await;
    let addr = endpoint.addr();
    let relay = addr
        .relay_urls()
        .next()
        .map_or_else(|| "-".to_owned(), ToString::to_string);
    let loopback = endpoint
        .bound_sockets()
        .iter()
        .find(|socket| socket.is_ipv4())
        .map_or_else(
            || "-".to_owned(),
            |socket| format!("127.0.0.1:{}", socket.port()),
        );
    let ips: Vec<String> = addr.ip_addrs().map(ToString::to_string).collect();
    println!(
        "ready id={} relay={relay} loopback={loopback} ips={}",
        endpoint.id(),
        ips.join(";")
    );
    while let Some(incoming) = endpoint.accept().await {
        tokio::spawn(iroh_echo_connection(incoming));
    }
    Ok(())
}

async fn iroh_echo_connection(incoming: Incoming) {
    let Ok(connection) = incoming.await else {
        return;
    };
    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut recv, &mut send).await;
        });
    }
}

fn target(args: &Args) -> io::Result<EndpointAddr> {
    let remote = args
        .remote
        .as_deref()
        .ok_or_else(|| io::Error::other(USAGE))?;
    let mut addr = EndpointAddr::new(decode_endpoint_id(remote)?);
    if let Some(relay) = &args.relay {
        addr = addr.with_relay_url(RelayUrl::from_str(relay).map_err(io::Error::other)?);
    }
    if let Some(ip) = &args.ip {
        addr = addr.with_ip_addr(ip.parse().map_err(io::Error::other)?);
    }
    Ok(addr)
}

// Warm samples reuse the endpoint. Seer does not do this today, so the warm
// rows show what endpoint reuse would save.
async fn iroh_dial(args: &Args, report: &Report) -> io::Result<()> {
    let target = target(args)?;
    let Some(endpoint) = report
        .step(0, "endpoint_bind", bind(args.relay_only()))
        .await
    else {
        return Ok(());
    };
    for sample in 0..args.samples {
        let connect = endpoint.connect_with_opts(target.clone(), ALPN, Default::default());
        let Some(connecting) = report.step(sample, "address_lookup", connect).await else {
            continue;
        };
        let Some(connection) = report.step(sample, "dial_handshake", connecting).await else {
            continue;
        };
        iroh_echoes(report, sample, &connection, args.echoes).await;
        connection.close(0_u8.into(), b"probe done");
    }
    endpoint.close().await;
    Ok(())
}

async fn iroh_echoes(report: &Report, sample: usize, connection: &Connection, echoes: usize) {
    let Some((mut send, mut recv)) = report
        .step(sample, "stream_open", open_and_echo(connection))
        .await
    else {
        return;
    };
    for _ in 0..echoes {
        let echo = echo_once(&mut send, &mut recv);
        if report.step(sample, "echo_rtt", echo).await.is_none() {
            return;
        }
    }
}

async fn open_and_echo(connection: &Connection) -> io::Result<(SendStream, RecvStream)> {
    let (mut send, mut recv) = connection.open_bi().await.map_err(io::Error::other)?;
    echo_once(&mut send, &mut recv).await?;
    Ok((send, recv))
}

async fn echo_once(send: &mut SendStream, recv: &mut RecvStream) -> io::Result<()> {
    send.write_all(b"x").await.map_err(io::Error::other)?;
    let mut byte = [0_u8; 1];
    recv.read_exact(&mut byte).await.map_err(io::Error::other)
}

fn seer_listen(args: &Args, report: &Report) {
    for sample in 0..args.samples {
        report.step_blocking(sample, "seer_listen_ready", || {
            Listener::bind(SecretKey::generate())
        });
    }
}

fn seer_serve() -> io::Result<()> {
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
fn seer_dial(args: &Args, report: &Report) -> io::Result<()> {
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
fn seer_session(args: &Args, report: &Report) -> io::Result<()> {
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
    unix_echo(&mut stream)?;
    Ok(stream)
}

fn unix_echoes(report: &Report, sample: usize, stream: Option<UnixStream>, echoes: usize) {
    let Some(mut stream) = stream else {
        return;
    };
    for _ in 0..echoes {
        if report
            .step_blocking(sample, "echo_rtt", || unix_echo(&mut stream))
            .is_none()
        {
            return;
        }
    }
}

fn unix_echo(stream: &mut UnixStream) -> io::Result<()> {
    stream.write_all(b"x")?;
    let mut byte = [0_u8; 1];
    stream.read_exact(&mut byte)
}

// Measurement probe for issue 389. The method and the boundaries are in
// docs/research/18-transport-baseline.md. scripts/perf/transport.sh drives it.

mod counters;
mod iroh_api;
mod seer_api;

use counters::counters;
use iroh::{EndpointAddr, RelayUrl};
use iroh_api::run_iroh;
use seer_api::{seer_dial, seer_listen, seer_serve, seer_session};
use seer_net::decode_endpoint_id;
use std::env;
use std::ffi::OsString;
use std::fmt::Display;
use std::fs;
use std::io;
use std::process::ExitCode;
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use tokio::time::timeout;

const STEP_TIMEOUT: Duration = Duration::from_secs(15);
const USAGE: &str = "usage: transport_probe serve|listen|dial|session --api iroh|seer \
[--condition default|loopback|lan|relay|discovery] [--samples N] [--echoes N] \
[--remote ID] [--relay URL] [--ip ADDR] [--ready online|bind] [--payload BYTES] \
[--counters 1]";

struct Args {
    command: String,
    api: String,
    condition: String,
    samples: usize,
    echoes: usize,
    remote: Option<String>,
    relay: Option<String>,
    ip: Option<String>,
    ready: String,
    payload: usize,
    counters: bool,
}

impl Args {
    fn relay_only(&self) -> bool {
        self.condition == "relay"
    }
}

struct Report {
    workload: String,
    condition: String,
    payload: Vec<u8>,
    counters: bool,
}

impl Report {
    fn line(&self, sample: usize, boundary: &str, ms: f64, status: &str) {
        let cache = if sample == 0 { "cold" } else { "warm" };
        println!(
            "{},{},{cache},{sample},{boundary},{ms:.3},{status}",
            self.workload, self.condition
        );
    }

    // Not a sample line. A thread that exits in the window loses its share.
    fn counters(&self, sample: usize, echoes: usize, before: &[(OsString, [u64; 4])]) {
        let mut sum = [0_u64; 4];
        for (task, after) in counters() {
            let old = before
                .iter()
                .find(|(id, _)| *id == task)
                .map_or([0; 4], |t| t.1);
            (0..4).for_each(|i| sum[i] += after[i].saturating_sub(old[i]));
        }
        let deltas: String = sum.iter().map(|value| format!(",{value}")).collect();
        let threads = fs::read_dir("/proc/self/task").map_or(0, Iterator::count);
        let (workload, condition, bytes) = (&self.workload, &self.condition, self.payload.len());
        if self.counters {
            println!("counters,{workload},{condition},{sample},{echoes},{bytes}{deltas},{threads}");
        }
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
    let size = if args.payload == 1 {
        String::new()
    } else {
        format!("_p{}", args.payload)
    };
    let report = Report {
        workload: format!("{}_{}{size}", args.api, args.command),
        condition: args.condition.clone(),
        payload: vec![b'x'; args.payload],
        counters: args.counters,
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
        ready: "online".to_owned(),
        payload: 1,
        counters: false,
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
            "--ready" => args.ready = value,
            "--payload" => args.payload = value.parse().map_err(|_| USAGE)?,
            "--counters" => args.counters = value == "1",
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
        ("serve" | "listen" | "dial" | "session", "iroh") => {
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

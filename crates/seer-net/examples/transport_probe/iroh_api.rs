use crate::counters::counters;
use crate::{Args, Report, target};
use iroh::endpoint::{Connection, Incoming, RecvStream, SendStream, presets};
use iroh::{Endpoint, SecretKey};
use seer_net::ALPN;
use std::io;

pub(crate) async fn run_iroh(args: &Args, report: &Report) -> io::Result<()> {
    match args.command.as_str() {
        "serve" => iroh_serve(args).await,
        "listen" => {
            iroh_listen(args, report).await;
            Ok(())
        }
        "session" => iroh_session(args, report).await,
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
    // With --ready bind, serve announces before the relay is online, as the
    // early readiness experiment in issue 386 did.
    if args.ready != "bind" {
        endpoint.online().await;
    }
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

// Sample 0 pays for the connection. Warm samples open a new stream on it, so
// the relay condition shows what connection reuse saves over endpoint reuse.
async fn iroh_session(args: &Args, report: &Report) -> io::Result<()> {
    let target = target(args)?;
    let Some(endpoint) = report
        .step(0, "endpoint_bind", bind(args.relay_only()))
        .await
    else {
        return Ok(());
    };
    let connect = endpoint.connect_with_opts(target, ALPN, Default::default());
    if let Some(connecting) = report.step(0, "address_lookup", connect).await
        && let Some(connection) = report.step(0, "dial_handshake", connecting).await
    {
        for sample in 0..args.samples {
            iroh_echoes(report, sample, &connection, args.echoes).await;
        }
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
    let before = counters();
    for _ in 0..echoes {
        let echo = echo_once(&mut send, &mut recv, &report.payload);
        if report.step(sample, "echo_rtt", echo).await.is_none() {
            return;
        }
    }
    report.counters(sample, echoes, &before);
}

async fn open_and_echo(connection: &Connection) -> io::Result<(SendStream, RecvStream)> {
    let (mut send, mut recv) = connection.open_bi().await.map_err(io::Error::other)?;
    echo_once(&mut send, &mut recv, b"x").await?;
    Ok((send, recv))
}

async fn echo_once(send: &mut SendStream, recv: &mut RecvStream, data: &[u8]) -> io::Result<()> {
    send.write_all(data).await.map_err(io::Error::other)?;
    let mut back = vec![0_u8; data.len()];
    recv.read_exact(&mut back).await.map_err(io::Error::other)
}

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::process::Command;

use serde_json::Value;

const INSTALL_MESSAGE: &str = "This server is on Tailscale. Install Tailscale from https://tailscale.com/download, sign in, and ask the owner to invite you to their network. Then run this command again.";
const CONNECT_MESSAGE: &str =
    "Tailscale is installed but not connected. Run tailscale up, then run this command again.";
const INVITE_MESSAGE: &str = "Tailscale is connected but cannot see this server. Ask the owner to invite you to their Tailscale network, then run this command again.";

pub(crate) enum CheckError {
    Action(&'static str),
    System(String),
}

pub(crate) fn check(endpoint: &str) -> Result<(), CheckError> {
    let Some(address) = tailscale_address(endpoint) else {
        return Ok(());
    };
    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .map_err(command_error)?;
    let status: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| CheckError::System(error.to_string()))?;
    if status.get("BackendState").and_then(Value::as_str) != Some("Running") {
        return Err(CheckError::Action(CONNECT_MESSAGE));
    }
    if !peer_addresses(&status).any(|peer| peer == address) {
        return Err(CheckError::Action(INVITE_MESSAGE));
    }
    Ok(())
}

fn command_error(error: io::Error) -> CheckError {
    if error.kind() == io::ErrorKind::NotFound {
        CheckError::Action(INSTALL_MESSAGE)
    } else {
        CheckError::System(error.to_string())
    }
}

fn tailscale_address(endpoint: &str) -> Option<Ipv4Addr> {
    let SocketAddr::V4(address) = endpoint.parse().ok()? else {
        return None;
    };
    let address = *address.ip();
    let octets = address.octets();
    (octets[0] == 100 && (64..=127).contains(&octets[1])).then_some(address)
}

fn peer_addresses(status: &Value) -> impl Iterator<Item = Ipv4Addr> + '_ {
    status
        .get("Peer")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|peers| peers.values())
        .filter_map(|peer| peer.get("TailscaleIPs").and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|address| address.parse().ok())
}

use std::net::SocketAddr;
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::{fs, path::Path};

#[cfg(target_os = "linux")]
pub(super) fn owns_listen_socket(pid: i32, listen: SocketAddr) -> bool {
    let Ok(entries) = fs::read_dir(format!("/proc/{pid}/fd")) else {
        return false;
    };
    let listening = listening_inodes(listen);
    entries.filter_map(Result::ok).any(|entry| {
        fs::read_link(entry.path())
            .ok()
            .and_then(|target| socket_inode(&target))
            .is_some_and(|inode| listening.contains(&inode))
    })
}
#[cfg(target_os = "linux")]
fn listening_inodes(listen: SocketAddr) -> Vec<String> {
    let (table, bytes) = match listen.ip() {
        std::net::IpAddr::V4(ip) => ("/proc/net/tcp", ip.octets().to_vec()),
        std::net::IpAddr::V6(ip) => ("/proc/net/tcp6", ip.octets().to_vec()),
    };
    let ip: String = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| format!("{:08X}", u32::from_ne_bytes(*word)))
        .collect();
    let address = format!("{ip}:{:04X}", listen.port());
    fs::read_to_string(table)
        .unwrap_or_default()
        .lines()
        .skip(1)
        .filter_map(|line| listening_inode(line, &address))
        .collect()
}
#[cfg(target_os = "linux")]
fn listening_inode(line: &str, address: &str) -> Option<String> {
    let fields: Vec<_> = line.split_whitespace().collect();
    if fields.get(1) == Some(&address) && fields.get(3) == Some(&"0A") {
        fields.get(9).map(|inode| (*inode).to_owned())
    } else {
        None
    }
}
#[cfg(target_os = "linux")]
fn socket_inode(path: &Path) -> Option<String> {
    path.to_str()?
        .strip_prefix("socket:[")?
        .strip_suffix(']')
        .map(str::to_owned)
}
#[cfg(target_os = "macos")]
pub(super) fn owns_listen_socket(pid: i32, listen: SocketAddr) -> bool {
    Command::new("/usr/sbin/lsof")
        .args([
            "-nP",
            "-a",
            "-p",
            &pid.to_string(),
            &format!("-i{}TCP@{listen}", if listen.is_ipv4() { 4 } else { 6 }),
            "-sTCP:LISTEN",
            "-Fp",
        ])
        .output()
        .is_ok_and(|output| output.status.success())
}

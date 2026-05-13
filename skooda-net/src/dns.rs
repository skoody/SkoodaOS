use std::net::Ipv4Addr;
use std::fs;
use skooda_utils::error::{Result, SkoodaError};
use tracing::info;

pub fn write_resolv_conf(dns_server: Ipv4Addr) -> Result<()> {
    let content = format!("nameserver {}\n", dns_server);
    info!("[dns] Writing /etc/resolv.conf with nameserver {}", dns_server);
    fs::write("/etc/resolv.conf", content).map_err(|e| SkoodaError::Io {
        path: "/etc/resolv.conf".into(),
        source: e,
    })
}

pub fn _dns_lookup(_name: &str, _dns_server: Ipv4Addr) -> Result<Ipv4Addr> {
    // Keep a placeholder for now, focused on resolv.conf for the daemon
    Err(SkoodaError::Network("DNS lookup not yet implemented in async mode".into()))
}

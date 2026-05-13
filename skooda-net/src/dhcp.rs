use std::net::Ipv4Addr;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
use skooda_utils::error::{Result, SkoodaError};
use tracing::info;

const DHCP_SERVER_PORT: u16 = 67;
const DHCP_CLIENT_PORT: u16 = 68;
const DHCP_MAGIC: [u8; 4] = [99, 130, 83, 99];

const DHCPDISCOVER: u8 = 1;
const DHCPOFFER: u8 = 2;
const DHCPREQUEST: u8 = 3;
const DHCPACK: u8 = 5;

#[derive(Debug, Clone)]
pub struct DhcpLease {
    pub ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub gateway: Ipv4Addr,
    pub dns: Ipv4Addr,
}

pub async fn dhcp_request(iface: &str) -> Result<DhcpLease> {
    let mac = get_mac(iface).ok_or_else(|| SkoodaError::System("Failed to read MAC".into()))?;
    crate::interface::set_interface_up(iface)?;

    // Use tokio's UdpSocket
    let sock = UdpSocket::bind(("0.0.0.0", DHCP_CLIENT_PORT)).await.map_err(|e| SkoodaError::Io {
        path: "DHCP Socket".into(),
        source: e,
    })?;
    sock.set_broadcast(true).map_err(|e| SkoodaError::Io {
        path: "DHCP Broadcast".into(),
        source: e,
    })?;

    let xid: u32 = std::process::id() ^ 0xDEAD_BEEF;

    info!("[dhcp] Sending DISCOVER on {}...", iface);
    let discover = build_dhcp_packet(DHCPDISCOVER, xid, &mac, None);
    sock.send_to(&discover, ("255.255.255.255", DHCP_SERVER_PORT)).await.map_err(|e| SkoodaError::Io {
        path: "DHCP Send".into(),
        source: e,
    })?;

    info!("[dhcp] Waiting for OFFER...");
    let mut buf = [0u8; 1500];
    let offer = match timeout(Duration::from_secs(5), recv_dhcp(&sock, &mut buf, xid, DHCPOFFER)).await {
        Ok(Ok(res)) => res,
        _ => return Err(SkoodaError::Network("DHCP OFFER timeout".into())),
    };

    info!("[dhcp] Got OFFER: {}", offer.0);
    let request = build_dhcp_packet(DHCPREQUEST, xid, &mac, Some(offer.0));
    sock.send_to(&request, ("255.255.255.255", DHCP_SERVER_PORT)).await.map_err(|e| SkoodaError::Io {
        path: "DHCP Send Request".into(),
        source: e,
    })?;

    info!("[dhcp] Waiting for ACK...");
    let ack = match timeout(Duration::from_secs(5), recv_dhcp(&sock, &mut buf, xid, DHCPACK)).await {
        Ok(Ok(res)) => res,
        _ => return Err(SkoodaError::Network("DHCP ACK timeout".into())),
    };

    let lease = DhcpLease {
        ip: ack.0,
        netmask: ack.1,
        gateway: ack.2,
        dns: ack.3,
    };

    crate::interface::set_ip(iface, lease.ip)?;
    crate::interface::set_netmask(iface, lease.netmask)?;

    info!("[dhcp] Lease acquired: IP={} GW={} DNS={}", lease.ip, lease.gateway, lease.dns);
    Ok(lease)
}

async fn recv_dhcp(sock: &UdpSocket, buf: &mut [u8], xid: u32, expected_type: u8) -> Result<(Ipv4Addr, Ipv4Addr, Ipv4Addr, Ipv4Addr)> {
    loop {
        let (n, _) = sock.recv_from(buf).await.map_err(|e| SkoodaError::Io {
            path: "DHCP Recv".into(),
            source: e,
        })?;

        let data = &buf[..n];
        if data.len() < 240 { continue; }

        let pkt_xid = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        if pkt_xid != xid { continue; }

        if let Some((msg_type, ip, mask, gw, dns)) = parse_dhcp_response(data) {
            if msg_type == expected_type {
                return Ok((ip, mask, gw, dns));
            }
        }
    }
}

fn build_dhcp_packet(msg_type: u8, xid: u32, mac: &[u8; 6], requested_ip: Option<Ipv4Addr>) -> Vec<u8> {
    let mut pkt = vec![0u8; 300];
    pkt[0] = 1; pkt[1] = 1; pkt[2] = 6;
    pkt[4..8].copy_from_slice(&xid.to_be_bytes());
    pkt[28..34].copy_from_slice(mac);
    pkt[236..240].copy_from_slice(&DHCP_MAGIC);

    let mut idx = 240;
    pkt[idx] = 53; pkt[idx+1] = 1; pkt[idx+2] = msg_type; idx += 3;
    if let Some(ip) = requested_ip {
        pkt[idx] = 50; pkt[idx+1] = 4;
        pkt[idx+2..idx+6].copy_from_slice(&ip.octets());
        idx += 6;
    }
    pkt[idx] = 55; pkt[idx+1] = 3; pkt[idx+2] = 1; pkt[idx+3] = 3; pkt[idx+4] = 6; idx += 5;
    pkt[idx] = 255;
    pkt
}

fn parse_dhcp_response(data: &[u8]) -> Option<(u8, Ipv4Addr, Ipv4Addr, Ipv4Addr, Ipv4Addr)> {
    if data[236..240] != DHCP_MAGIC { return None; }
    let your_ip = Ipv4Addr::new(data[16], data[17], data[18], data[19]);
    let mut msg_type = 0;
    let mut mask = Ipv4Addr::new(255, 255, 255, 0);
    let mut gw = Ipv4Addr::new(0,0,0,0);
    let mut dns = Ipv4Addr::new(0,0,0,0);

    let mut i = 240;
    while i < data.len() {
        let opt = data[i];
        if opt == 255 { break; }
        if opt == 0 { i += 1; continue; }
        let len = data[i+1] as usize;
        let v = i + 2;
        match opt {
            53 => msg_type = data[v],
            1 if len == 4 => mask = Ipv4Addr::new(data[v], data[v+1], data[v+2], data[v+3]),
            3 if len >= 4 => gw = Ipv4Addr::new(data[v], data[v+1], data[v+2], data[v+3]),
            6 if len >= 4 => dns = Ipv4Addr::new(data[v], data[v+1], data[v+2], data[v+3]),
            _ => {}
        }
        i = v + len;
    }
    Some((msg_type, your_ip, mask, gw, dns))
}

fn get_mac(iface: &str) -> Option<[u8; 6]> {
    let path = format!("/sys/class/net/{}/address", iface);
    let mac_str = std::fs::read_to_string(path).ok()?;
    let parts: Vec<u8> = mac_str.trim().split(':').filter_map(|s| u8::from_str_radix(s, 16).ok()).collect();
    if parts.len() != 6 { return None; }
    let mut mac = [0u8; 6]; mac.copy_from_slice(&parts);
    Some(mac)
}

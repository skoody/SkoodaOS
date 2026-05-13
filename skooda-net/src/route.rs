use std::net::Ipv4Addr;
use skooda_utils::error::{Result, SkoodaError};
use tracing::{info, warn};

#[repr(C)]
struct RtEntry {
    pub rt_pad1: libc::c_ulong,
    pub rt_dst: libc::sockaddr,
    pub rt_gateway: libc::sockaddr,
    pub rt_genmask: libc::sockaddr,
    pub rt_flags: libc::c_ushort,
    pub rt_pad2: libc::c_short,
    pub rt_pad3: libc::c_ulong,
    pub rt_tos: libc::c_uchar,
    pub rt_class: libc::c_uchar,
    pub rt_pad4: [libc::c_short; 3],
    pub rt_metric: libc::c_short,
    pub rt_dev: *mut libc::c_char,
    pub rt_mtu: libc::c_ulong,
    pub rt_window: libc::c_ulong,
    pub rt_irtt: libc::c_ushort,
}

pub fn add_default_route(iface: &str, gateway: Ipv4Addr) -> Result<()> {
    let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if sock < 0 {
        return Err(SkoodaError::System("Failed to create socket".into()));
    }

    let mut rt: RtEntry = unsafe { std::mem::zeroed() };

    // Destination: 0.0.0.0
    let dst = unsafe { &mut *(&mut rt.rt_dst as *mut _ as *mut libc::sockaddr_in) };
    dst.sin_family = libc::AF_INET as u16;
    dst.sin_addr.s_addr = 0;

    // Mask: 0.0.0.0
    let mask = unsafe { &mut *(&mut rt.rt_genmask as *mut _ as *mut libc::sockaddr_in) };
    mask.sin_family = libc::AF_INET as u16;
    mask.sin_addr.s_addr = 0;

    // Gateway
    let gw = unsafe { &mut *(&mut rt.rt_gateway as *mut _ as *mut libc::sockaddr_in) };
    gw.sin_family = libc::AF_INET as u16;
    gw.sin_addr.s_addr = u32::from_ne_bytes(gateway.octets());

    rt.rt_flags = (libc::RTF_UP | libc::RTF_GATEWAY) as u16;
    
    let iface_c = std::ffi::CString::new(iface).unwrap();
    rt.rt_dev = iface_c.as_ptr() as *mut _;

    let ret = unsafe { libc::ioctl(sock, libc::SIOCADDRT.try_into().unwrap(), &rt) };
    unsafe { libc::close(sock) };

    if ret < 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::AlreadyExists {
            info!("[route] Default route already exists for {}", iface);
            return Ok(());
        }
        return Err(SkoodaError::Network(format!("SIOCADDRT failed for {}: {}", iface, err)));
    }

    info!("[route] Default gateway for {} set to {}", iface, gateway);
    Ok(())
}

pub fn delete_default_route(iface: &str) -> Result<()> {
    let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if sock < 0 {
        return Err(SkoodaError::System("Failed to create socket".into()));
    }

    let mut rt: RtEntry = unsafe { std::mem::zeroed() };
    
    // Destination: 0.0.0.0
    let dst = unsafe { &mut *(&mut rt.rt_dst as *mut _ as *mut libc::sockaddr_in) };
    dst.sin_family = libc::AF_INET as u16;
    dst.sin_addr.s_addr = 0;

    let iface_c = std::ffi::CString::new(iface).unwrap();
    rt.rt_dev = iface_c.as_ptr() as *mut _;

    let ret = unsafe { libc::ioctl(sock, libc::SIOCDELRT.try_into().unwrap(), &rt) };
    unsafe { libc::close(sock) };

    if ret < 0 {
        let err = std::io::Error::last_os_error();
        warn!("[route] SIOCDELRT failed for {}: {}", iface, err);
    } else {
        info!("[route] Default route for {} removed", iface);
    }
    Ok(())
}

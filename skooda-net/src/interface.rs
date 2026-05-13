use std::net::Ipv4Addr;
use skooda_utils::error::{Result, SkoodaError};

const IFNAMSIZ: usize = 16;

#[repr(C)]
pub struct Ifreq {
    pub ifr_name: [u8; IFNAMSIZ],
    pub ifr_data: [u8; 24],
}

impl Ifreq {
    pub fn new(name: &str) -> Self {
        let mut ifr = Ifreq {
            ifr_name: [0u8; IFNAMSIZ],
            ifr_data: [0u8; 24],
        };
        let bytes = name.as_bytes();
        let len = bytes.len().min(IFNAMSIZ - 1);
        ifr.ifr_name[..len].copy_from_slice(&bytes[..len]);
        ifr
    }

    pub fn set_addr(&mut self, ip: Ipv4Addr) {
        self.ifr_data[0] = libc::AF_INET as u8;
        let octets = ip.octets();
        self.ifr_data[4] = octets[0];
        self.ifr_data[5] = octets[1];
        self.ifr_data[6] = octets[2];
        self.ifr_data[7] = octets[3];
    }

    pub fn get_flags(&self) -> i16 {
        i16::from_ne_bytes([self.ifr_data[0], self.ifr_data[1]])
    }

    pub fn set_flags(&mut self, flags: i16) {
        let bytes = flags.to_ne_bytes();
        self.ifr_data[0] = bytes[0];
        self.ifr_data[1] = bytes[1];
    }
}

pub fn set_interface_up(name: &str) -> Result<()> {
    let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if sock < 0 {
        return Err(SkoodaError::System("Failed to create socket".into()));
    }

    let mut ifr = Ifreq::new(name);
    let ret = unsafe { libc::ioctl(sock, libc::SIOCGIFFLAGS.try_into().unwrap(), &mut ifr) };
    if ret < 0 {
        unsafe { libc::close(sock) };
        return Err(SkoodaError::Network(format!("SIOCGIFFLAGS failed for {}", name)));
    }

    let flags = ifr.get_flags() | libc::IFF_UP as i16 | libc::IFF_RUNNING as i16;
    ifr.set_flags(flags);

    let ret = unsafe { libc::ioctl(sock, libc::SIOCSIFFLAGS.try_into().unwrap(), &ifr) };
    unsafe { libc::close(sock) };

    if ret < 0 {
        return Err(SkoodaError::Network(format!("SIOCSIFFLAGS failed for {}", name)));
    }
    Ok(())
}

pub fn set_ip(name: &str, ip: Ipv4Addr) -> Result<()> {
    let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if sock < 0 {
        return Err(SkoodaError::System("Failed to create socket".into()));
    }

    let mut ifr = Ifreq::new(name);
    ifr.set_addr(ip);

    let ret = unsafe { libc::ioctl(sock, libc::SIOCSIFADDR.try_into().unwrap(), &ifr) };
    unsafe { libc::close(sock) };

    if ret < 0 {
        return Err(SkoodaError::Network(format!("SIOCSIFADDR failed for {}", name)));
    }
    Ok(())
}

pub fn set_netmask(name: &str, mask: Ipv4Addr) -> Result<()> {
    let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if sock < 0 {
        return Err(SkoodaError::System("Failed to create socket".into()));
    }

    let mut ifr = Ifreq::new(name);
    ifr.set_addr(mask);

    let ret = unsafe { libc::ioctl(sock, libc::SIOCSIFNETMASK.try_into().unwrap(), &ifr) };
    unsafe { libc::close(sock) };

    if ret < 0 {
        return Err(SkoodaError::Network(format!("SIOCSIFNETMASK failed for {}", name)));
    }
    Ok(())
}

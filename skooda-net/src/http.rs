use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpStream};

pub fn http_get(url: &str, dns_server: Ipv4Addr) -> Result<String, String> {
    let url = url.strip_prefix("http://").unwrap_or(url);
    let (host, path) = match url.find('/') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, "/"),
    };

    let (connect_host, port) = match host.find(':') {
        Some(i) => (&host[..i], host[i+1..].parse::<u16>().unwrap_or(80)),
        None => (host, 80u16),
    };

    let ip: Ipv4Addr = if let Ok(parsed) = connect_host.parse() {
        parsed
    } else {
        println!("[http] Resolving {}...", connect_host);
        crate::dns::dns_lookup(connect_host, dns_server)?
    };

    println!("[http] Connecting to {}:{}...", ip, port);
    let mut stream = TcpStream::connect(format!("{}:{}", ip, port))
        .map_err(|e| format!("TCP connect failed: {}", e))?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: SkoodaOS/1.0\r\n\r\n",
        path, connect_host
    );
    stream.write_all(request.as_bytes())
        .map_err(|e| format!("Write failed: {}", e))?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)
        .map_err(|e| format!("Read failed: {}", e))?;

    let text = String::from_utf8_lossy(&response).to_string();

    if let Some(body_start) = text.find("\r\n\r\n") {
        Ok(text[body_start + 4..].to_string())
    } else {
        Ok(text)
    }
}

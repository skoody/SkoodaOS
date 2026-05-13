use std::fs::OpenOptions;
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;

fn main() -> io::Result<()> {
    // Open /dev/kmsg in non-blocking mode
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/kmsg")?;

    let mut buffer = [0u8; 8192];
    
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                // /dev/kmsg format is: priority,sequence,timestamp,flags;message
                // We print it as is for now
                print!("{}", String::from_utf8_lossy(&buffer[..n]));
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                break;
            }
            Err(e) => {
                eprintln!("\n[dmesg error] {}", e);
                break;
            }
        }
    }
    
    Ok(())
}

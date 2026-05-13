use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;

#[repr(C)]
struct KbEntry {
    kb_table: u8,
    kb_index: u8,
    kb_value: u16,
}

const KDSKBENT: libc::c_ulong = 0x4B47;

fn set_key(fd: i32, table: u8, index: u8, value: u16) {
    let entry = KbEntry {
        kb_table: table,
        kb_index: index,
        kb_value: value,
    };
    unsafe {
        libc::ioctl(fd, KDSKBENT as _, &entry);
    }
}

fn main() {
    let tty = OpenOptions::new().write(true).open("/dev/tty0").expect("Failed to open /dev/tty0");
    let fd = tty.as_raw_fd();

    println!("Setting German keyboard layout (QWERTZ)...");

    // Table 0: Normal
    // Table 1: Shift
    // Table 2: AltGr
    
    // Y (Key 21 in US is Z) -> 'z' (0x007a)
    set_key(fd, 0, 21, 0x007a);
    set_key(fd, 1, 21, 0x005a); // Z

    // Z (Key 44 in US is Y) -> 'y' (0x0079)
    set_key(fd, 0, 44, 0x0079);
    set_key(fd, 1, 44, 0x0059); // Y

    // Add '-' (Key 12) and other common ones if needed
    // This is a minimal set to make it usable
    
    println!("Done.");
}

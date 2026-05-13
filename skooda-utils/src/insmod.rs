use std::fs::File;
use std::io::Read;
use std::env;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: insmod <module.ko>");
        std::process::exit(1);
    }

    let path = &args[1];
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    // init_module syscall
    let params = std::ffi::CString::new("").unwrap();
    let res = unsafe {
        libc::syscall(
            libc::SYS_init_module,
            buffer.as_ptr(),
            buffer.len() as libc::size_t,
            params.as_ptr(),
        )
    };

    if res != 0 {
        let err = std::io::Error::last_os_error();
        eprintln!("Error loading module {}: {}", path, err);
        std::process::exit(1);
    }

    println!("Module {} loaded successfully.", path);
    Ok(())
}

use std::fs;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = if args.len() > 1 { &args[1] } else { "." };

    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let name = entry.file_name().into_string().unwrap();
                let metadata = entry.metadata().unwrap();
                if metadata.is_dir() {
                    print!("\x1b[1;34m{}\x1b[0m  ", name); // Blue for dirs
                } else {
                    print!("{}  ", name);
                }
            }
            println!();
        }
        Err(e) => eprintln!("ls: cannot access '{}': {}", path, e),
    }
}

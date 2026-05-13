use std::env;
use skooda_utils::fs::mkdir;

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Some(path) = args.get(1) {
        if let Err(e) = mkdir(path) {
            eprintln!("mkdir: {}: {}", path, e);
        }
    } else {
        eprintln!("Usage: mkdir <dir>");
    }
}

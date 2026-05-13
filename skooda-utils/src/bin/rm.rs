use std::env;
use skooda_utils::fs::rm;

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Some(path) = args.get(1) {
        if let Err(e) = rm(path) {
            eprintln!("rm: {}: {}", path, e);
        }
    } else {
        eprintln!("Usage: rm <file>");
    }
}

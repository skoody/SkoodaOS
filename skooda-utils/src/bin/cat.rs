use std::env;
use skooda_utils::fs::cat;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { return; }
    for path in &args[1..] {
        match cat(path) {
            Ok(content) => print!("{}", content),
            Err(e) => eprintln!("cat: {}: {}", path, e),
        }
    }
}

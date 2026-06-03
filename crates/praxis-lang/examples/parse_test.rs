//! Quick .px parser test binary
use praxis_lang::parse;
fn main() {
    for path in std::env::args().skip(1) {
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("Cannot read {}: {}", path, e);
            std::process::exit(1);
        });
        match parse(&src) {
            Ok(_) => println!("✅ {}", path),
            Err(e) => println!("❌ {}: {}", path, e),
        }
    }
}

//! Trivial long-lived fixture binary used by `supervisor` tests to stand in
//! for a real supervised plugin process without depending on the full
//! `pares-agens` binary.
fn main() {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

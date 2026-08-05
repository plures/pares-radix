//! Trivial fixture binary that writes its received argv (one arg per line,
//! excluding argv[0]) to a file named by the `PLUGIN_FIXTURE_ECHO_ARGV_OUT`
//! env var, then stays alive like `plugin-fixture-long-lived` so the
//! supervisor's health check (a "still running shortly after spawn" probe)
//! passes. Used by `supervisor_integration.rs` to prove the supervisor
//! actually passes real `args`/`envs` through to the spawned child process,
//! not just that `PluginSpawnRequest` was constructed correctly.
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Ok(out_path) = std::env::var("PLUGIN_FIXTURE_ECHO_ARGV_OUT") {
        if let Ok(mut f) = std::fs::File::create(&out_path) {
            for a in &args {
                let _ = writeln!(f, "{a}");
            }
            // Also record a marker env var so tests can prove `envs` was
            // actually passed through to the child, not just inherited.
            if let Ok(marker) = std::env::var("PLUGIN_FIXTURE_ECHO_ENV_MARKER") {
                let _ = writeln!(f, "ENV_MARKER={marker}");
            }
        }
    }
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

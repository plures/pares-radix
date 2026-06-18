//! Smoke harness: runs the SAME shadow-load path the `serve` startup hook uses
//! (`ShadowProcedures::load_dir`) against the real shadow dir, with tracing at
//! info level, so we can observe the actual runtime log line
//! ("N shadow candidates loaded ...") without standing up Telegram/PluresDB.
//! This is the production holder + production loader against real files — not a
//! mock.
//!
//! Dir resolution mirrors the `serve` daemon, whose WorkingDirectory is the
//! pares-radix home and which loads CWD-relative `praxis/shadow` (synced by the
//! nixos service from the package). Order: explicit arg, then CWD `praxis/shadow`,
//! then `~/praxis/shadow` (the praxisbot staged location).
//!
//! Run from the repo root: `cargo run -p pares-agens-core --example shadow_smoke`

use std::path::PathBuf;
use std::sync::Arc;

use pares_agens_core::px_adapter::{AsyncActionHandler, ToolDispatchActionHandler};
use pares_agens_core::spine::shadow::ShadowProcedures;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Resolve the shadow dir the way the live `serve` hook does (CWD-relative),
    // with an explicit-arg override and the praxisbot staged path as fallback.
    let shadow_dir = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        let cwd_rel = PathBuf::from("praxis/shadow");
        if cwd_rel.is_dir() {
            cwd_rel
        } else {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_default();
            PathBuf::from(&home).join("praxis/shadow")
        }
    });
    println!("RESULT: shadow_dir={}", shadow_dir.display());

    // Exactly mirrors the shadow-load hook in crates/cli/src/main.rs (serve).
    let handler: Arc<dyn AsyncActionHandler> = Arc::new(ToolDispatchActionHandler::new_lazy());
    let mut shadow = ShadowProcedures::new();
    let loaded = shadow.load_dir(&shadow_dir, handler);

    println!("RESULT: loaded={} names={:?}", loaded, {
        let mut n = shadow.names();
        n.sort();
        n
    });
    println!("RESULT: candidates={:?}", {
        let mut c: Vec<(String, String)> = shadow
            .candidates()
            .into_iter()
            .map(|x| (x.name, x.trigger_kind))
            .collect();
        c.sort();
        c
    });
}

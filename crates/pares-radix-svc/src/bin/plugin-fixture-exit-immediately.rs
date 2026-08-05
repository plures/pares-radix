//! Trivial fixture binary that exits immediately with a nonzero code, used
//! to exercise `PluginSupervisor`'s health-check failure path.
fn main() {
    std::process::exit(7);
}

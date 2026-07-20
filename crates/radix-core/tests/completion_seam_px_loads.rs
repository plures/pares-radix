//! Parse/compile guard for `praxis/procedures/task-completion-seam.px` (S0 of
//! the task-completion-seam epic). Mirrors briefing_px_loads.rs: loads the real
//! file via CARGO_MANIFEST_DIR and runs the actual parse→compile→adapter chain.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use pares_radix_core::px_adapter::{load_px_procedures, AsyncActionHandler};
use pares_radix_praxis::px::executor::ExecutionError;

struct NoopHandler;

#[async_trait]
impl AsyncActionHandler for NoopHandler {
    async fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
        Err(ExecutionError::UnknownAction(name.to_string()))
    }
}

fn seam_px_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("praxis")
        .join("procedures")
        .join("task-completion-seam.px")
}

#[test]
fn task_completion_seam_px_parses_and_compiles() {
    let path = seam_px_path();
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let handler: Arc<dyn AsyncActionHandler> = Arc::new(NoopHandler);
    let adapters = load_px_procedures(&source, handler)
        .unwrap_or_else(|e| panic!("task-completion-seam.px must parse+compile: {e}"));

    assert_eq!(
        adapters.len(),
        1,
        "expected exactly one procedure (finalize_task) to compile"
    );
}

//! Re-export of `pluresdb_px::db` — the constraint store and evaluation engine
//! now lives in the foundation layer (`pluresdb-px` crate) and is re-exported
//! here for backward compatibility.

pub use pluresdb_px::db::*;

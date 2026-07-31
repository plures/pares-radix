use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphStoreError {
    #[error("could not open PluresDB storage at {path}: {message}")]
    Open { path: String, message: String },

    #[error("could not serialize record {key}: {source}")]
    Serialize {
        key: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("persisted record {key} is invalid: {source}")]
    Deserialize {
        key: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("canonical graph hash failed: {0}")]
    Canonical(#[from] px_repo_model::canonical::CanonicalError),
}

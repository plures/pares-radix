//! Automatic model download and cache management.
//!
//! Downloads BitNet GGUF models from HuggingFace on first use, caching them
//! locally so subsequent starts are instant.

use std::path::{Path, PathBuf};

const DEFAULT_MODEL_REPO: &str = "microsoft/BitNet-b1.58-2B-4T-gguf";
const DEFAULT_MODEL_FILE: &str = "ggml-model-i2_s.gguf";
const HF_BASE_URL: &str = "https://huggingface.co";

/// Manages local model cache and downloads from HuggingFace.
pub struct ModelManager {
    cache_dir: PathBuf,
}

impl ModelManager {
    /// Create a new manager. Cache location is determined by:
    /// 1. `PARES_MODEL_CACHE` env var
    /// 2. `$HOME/.cache/pares-models`
    pub fn new() -> Self {
        let cache_dir = std::env::var("PARES_MODEL_CACHE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
                PathBuf::from(home).join(".cache").join("pares-models")
            });
        Self { cache_dir }
    }

    /// Returns path to the default BitNet model, downloading if not cached.
    pub async fn ensure_bitnet_model(&self) -> Result<PathBuf, ModelDownloadError> {
        let model_path = self.cache_dir.join("bitnet").join(DEFAULT_MODEL_FILE);

        if model_path.exists() {
            tracing::info!("BitNet model found at {}", model_path.display());
            return Ok(model_path);
        }

        tracing::info!("BitNet model not found locally, downloading from HuggingFace...");
        self.download_model(DEFAULT_MODEL_REPO, DEFAULT_MODEL_FILE, &model_path)
            .await?;
        Ok(model_path)
    }

    /// Check if a newer version is available (compares Content-Length).
    /// Returns `true` if an update is available or model is missing.
    /// Returns `false` if up-to-date or offline.
    pub async fn check_for_updates(&self) -> Result<bool, ModelDownloadError> {
        let model_path = self.cache_dir.join("bitnet").join(DEFAULT_MODEL_FILE);
        if !model_path.exists() {
            return Ok(true);
        }

        let url = format!(
            "{}/{}/resolve/main/{}",
            HF_BASE_URL, DEFAULT_MODEL_REPO, DEFAULT_MODEL_FILE
        );
        let client = reqwest::Client::new();
        match client.head(&url).send().await {
            Ok(resp) => {
                if let Some(remote_size) = resp
                    .headers()
                    .get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    let local_size = std::fs::metadata(&model_path).map(|m| m.len()).unwrap_or(0);
                    Ok(remote_size != local_size)
                } else {
                    Ok(false)
                }
            }
            Err(_) => Ok(false), // offline, skip
        }
    }

    /// Returns the cache directory path.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    async fn download_model(
        &self,
        repo: &str,
        filename: &str,
        dest: &Path,
    ) -> Result<(), ModelDownloadError> {
        let url = format!("{}/{}/resolve/main/{}", HF_BASE_URL, repo, filename);

        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ModelDownloadError::Io(e.to_string()))?;
        }

        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ModelDownloadError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ModelDownloadError::HttpError(resp.status().as_u16()));
        }

        let total_size = resp.content_length().unwrap_or(0);
        tracing::info!(
            "Downloading BitNet model ({:.1} MB)...",
            total_size as f64 / 1_048_576.0
        );

        // Stream to a temp file then rename for atomicity
        let tmp_dest = dest.with_extension("gguf.part");
        let mut file = tokio::fs::File::create(&tmp_dest)
            .await
            .map_err(|e| ModelDownloadError::Io(e.to_string()))?;

        use tokio::io::AsyncWriteExt;
        let mut stream = resp.bytes_stream();
        use futures_util::StreamExt;
        let mut downloaded: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ModelDownloadError::Network(e.to_string()))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| ModelDownloadError::Io(e.to_string()))?;
            downloaded += chunk.len() as u64;
            if total_size > 0 && downloaded % (50 * 1024 * 1024) < chunk.len() as u64 {
                tracing::info!(
                    "Download progress: {:.0}%",
                    (downloaded as f64 / total_size as f64) * 100.0
                );
            }
        }
        file.flush()
            .await
            .map_err(|e| ModelDownloadError::Io(e.to_string()))?;
        drop(file);

        // Atomic rename
        tokio::fs::rename(&tmp_dest, dest)
            .await
            .map_err(|e| ModelDownloadError::Io(e.to_string()))?;

        tracing::info!("BitNet model downloaded to {}", dest.display());
        Ok(())
    }
}

impl Default for ModelManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during model download.
#[derive(Debug)]
pub enum ModelDownloadError {
    /// Network/connection error.
    Network(String),
    /// HTTP non-success status code.
    HttpError(u16),
    /// Filesystem I/O error.
    Io(String),
}

impl std::fmt::Display for ModelDownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "Network error: {e}"),
            Self::HttpError(code) => write!(f, "HTTP error: {code}"),
            Self::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for ModelDownloadError {}

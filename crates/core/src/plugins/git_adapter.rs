//! Git adapter plugin — bridges git CLI operations with PluresDB storage.
//!
//! This is NOT a full git reimplementation. It wraps the git CLI to let the AI
//! import repos, track commits, create branches, and push — all while storing
//! project state in PluresDB as structured entities.

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

use crate::plugins::error::PluginError;

/// Git adapter — wraps git CLI and stores project state in PluresDB.
pub struct GitAdapter;

/// Result of a sync operation.
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub new_commits: usize,
    pub current_branch: String,
}

/// Current git working tree status.
#[derive(Debug, Clone)]
pub struct GitStatus {
    pub branch: String,
    pub modified: Vec<String>,
    pub staged: Vec<String>,
    pub untracked: Vec<String>,
    pub ahead: usize,
    pub behind: usize,
}

/// A parsed commit from `git log`.
#[derive(Debug, Clone)]
pub struct CommitEntry {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub timestamp: String,
    pub files_changed: usize,
}

impl GitAdapter {
    // ── Repo-level operations ────────────────────────────────────────────

    /// Import a git repo — gather metadata and recent commits.
    ///
    /// Returns `(repo_name, remote_url, branch, Vec<CommitEntry>)`.
    /// The caller is responsible for persisting these into PluresDB.
    pub async fn import_repo(
        path: &str,
    ) -> Result<(String, Option<String>, String, Vec<CommitEntry>), PluginError> {
        let repo_path = Path::new(path);
        if !repo_path.join(".git").exists() {
            return Err(PluginError::Storage(format!(
                "{path} is not a git repository"
            )));
        }

        let name = Self::run_git(path, &["rev-parse", "--show-toplevel"])
            .await
            .map(|p| {
                Path::new(p.trim())
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })?;

        let remote_url = Self::run_git(path, &["remote", "get-url", "origin"])
            .await
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let branch = Self::current_branch(path).await?;
        let commits = Self::recent_commits(path, 50).await?;

        Ok((name, remote_url, branch, commits))
    }

    /// Sync: pull and return new commits since `since_hash`.
    pub async fn sync_repo(
        path: &str,
        since_hash: Option<&str>,
    ) -> Result<SyncResult, PluginError> {
        Self::run_git(path, &["pull", "--ff-only"]).await?;
        let branch = Self::current_branch(path).await?;

        let commits = match since_hash {
            Some(hash) => {
                let range = format!("{hash}..HEAD");
                Self::commits_from_log(path, &["log", &range, "--format=%H%n%s%n%an%n%aI%n", "--numstat"])
                    .await?
            }
            None => Self::recent_commits(path, 50).await?,
        };

        Ok(SyncResult {
            new_commits: commits.len(),
            current_branch: branch,
        })
    }

    // ── Working-tree operations ──────────────────────────────────────────

    /// Stage all changes and commit. Returns the new commit hash.
    pub async fn commit(repo_path: &str, message: &str) -> Result<String, PluginError> {
        Self::run_git(repo_path, &["add", "-A"]).await?;
        Self::run_git(repo_path, &["commit", "-m", message]).await?;
        let hash = Self::run_git(repo_path, &["rev-parse", "HEAD"]).await?;
        Ok(hash.trim().to_string())
    }

    /// Create and checkout a new branch.
    pub async fn create_branch(repo_path: &str, branch_name: &str) -> Result<(), PluginError> {
        Self::run_git(repo_path, &["checkout", "-b", branch_name]).await?;
        Ok(())
    }

    /// Push current branch to origin.
    pub async fn push(repo_path: &str) -> Result<(), PluginError> {
        let branch = Self::current_branch(repo_path).await?;
        Self::run_git(repo_path, &["push", "-u", "origin", &branch]).await?;
        Ok(())
    }

    /// Get current working tree status.
    pub async fn status(repo_path: &str) -> Result<GitStatus, PluginError> {
        let branch = Self::current_branch(repo_path).await?;

        let porcelain =
            Self::run_git(repo_path, &["status", "--porcelain=v1"]).await?;

        let mut modified = Vec::new();
        let mut staged = Vec::new();
        let mut untracked = Vec::new();

        for line in porcelain.lines() {
            if line.len() < 4 {
                continue;
            }
            let index = line.as_bytes()[0];
            let worktree = line.as_bytes()[1];
            let file = line[3..].to_string();

            if index == b'?' {
                untracked.push(file);
            } else {
                if index != b' ' && index != b'?' {
                    staged.push(file.clone());
                }
                if worktree != b' ' && worktree != b'?' {
                    modified.push(file);
                }
            }
        }

        // ahead/behind
        let (ahead, behind) = Self::ahead_behind(repo_path).await.unwrap_or((0, 0));

        Ok(GitStatus {
            branch,
            modified,
            staged,
            untracked,
            ahead,
            behind,
        })
    }

    /// Get diff of current uncommitted changes.
    pub async fn diff(repo_path: &str) -> Result<String, PluginError> {
        Self::run_git(repo_path, &["diff"]).await
    }

    // ── Tool definitions ─────────────────────────────────────────────────

    /// Return tool definitions for the git adapter, suitable for system prompt
    /// injection.
    pub fn tool_names() -> Vec<&'static str> {
        vec![
            "git_import",
            "git_sync",
            "git_commit",
            "git_push",
            "git_status",
            "git_diff",
            "git_branch",
        ]
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    async fn current_branch(path: &str) -> Result<String, PluginError> {
        let branch = Self::run_git(path, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;
        Ok(branch.trim().to_string())
    }

    async fn ahead_behind(path: &str) -> Result<(usize, usize), PluginError> {
        let output =
            Self::run_git(path, &["rev-list", "--left-right", "--count", "HEAD...@{u}"])
                .await?;
        let parts: Vec<&str> = output.trim().split('\t').collect();
        if parts.len() == 2 {
            let ahead = parts[0].parse().unwrap_or(0);
            let behind = parts[1].parse().unwrap_or(0);
            Ok((ahead, behind))
        } else {
            Ok((0, 0))
        }
    }

    async fn recent_commits(
        path: &str,
        limit: usize,
    ) -> Result<Vec<CommitEntry>, PluginError> {
        let n = format!("-{limit}");
        Self::commits_from_log(
            path,
            &["log", &n, "--format=%H%n%s%n%an%n%aI%n", "--numstat"],
        )
        .await
    }

    async fn commits_from_log(
        path: &str,
        args: &[&str],
    ) -> Result<Vec<CommitEntry>, PluginError> {
        let output = Self::run_git(path, args).await?;
        let mut commits = Vec::new();
        let mut lines = output.lines().peekable();

        while let Some(hash) = lines.next() {
            let hash = hash.trim();
            if hash.is_empty() {
                continue;
            }
            // We expect: hash, message, author, timestamp, then numstat lines
            // until the next blank line or end.
            let message = lines.next().unwrap_or("").trim().to_string();
            let author = lines.next().unwrap_or("").trim().to_string();
            let timestamp = lines.next().unwrap_or("").trim().to_string();

            // Count numstat lines (file changes) until blank or next commit
            let mut files_changed = 0usize;
            while let Some(&line) = lines.peek() {
                if line.trim().is_empty() {
                    let _ = lines.next(); // consume blank separator
                    break;
                }
                // numstat lines look like: "3\t1\tfile.rs"
                if line.contains('\t') {
                    files_changed += 1;
                }
                let _ = lines.next();
            }

            if hash.len() >= 7 {
                commits.push(CommitEntry {
                    hash: hash.to_string(),
                    message,
                    author,
                    timestamp,
                    files_changed,
                });
            }
        }

        Ok(commits)
    }

    async fn run_git(cwd: &str, args: &[&str]) -> Result<String, PluginError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PluginError::Storage(format!("failed to run git: {e}")))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(PluginError::Storage(format!(
                "git {} failed: {stderr}",
                args.first().unwrap_or(&"")
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn status_on_real_repo() {
        // This test only works when run inside a git repo.
        let path = env!("CARGO_MANIFEST_DIR");
        if !Path::new(path).join(".git").exists()
            && !Path::new(path).join("../../.git").exists()
        {
            return; // skip if not in a git repo
        }
        // Use the workspace root
        let workspace_root = Path::new(path)
            .ancestors()
            .find(|p| p.join(".git").exists());
        if let Some(root) = workspace_root {
            let status = GitAdapter::status(root.to_str().unwrap()).await.unwrap();
            assert!(!status.branch.is_empty());
        }
    }

    #[test]
    fn tool_names_are_complete() {
        let names = GitAdapter::tool_names();
        assert_eq!(names.len(), 7);
        assert!(names.contains(&"git_import"));
        assert!(names.contains(&"git_status"));
    }
}

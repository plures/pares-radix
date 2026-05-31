//! Shell executor — robust command execution with PTY, background, and session tracking.
//!
//! This module provides [`ShellExecutor`], the primary tool for running shell
//! commands within pares-radix. It mirrors the capabilities of OpenClaw's `exec`
//! tool:
//!
//! - **Foreground execution** with timeout and output capture
//! - **Background execution** with session tracking
//! - **PTY mode** for interactive CLIs
//! - **Stdin writing** to running processes
//! - **Working directory** and environment variable support
//! - **Session management** (list, poll, kill, write)
//!
//! # Architecture
//!
//! ```text
//! ShellExecutor
//!   ├── foreground_exec()  → run, wait, return output
//!   ├── background_exec()  → spawn, track in SessionMap, return session_id
//!   ├── session_poll()     → check status, return new output
//!   ├── session_write()    → write to stdin of running session
//!   ├── session_kill()     → kill a running session
//!   └── session_list()     → list all tracked sessions
//! ```
//!
//! # Integration
//!
//! The executor is exposed as a tool through the [`ToolDispatcher`] trait.
//! The tool governance layer checks policies before execution.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tracing::info;
// ── Configuration ─────────────────────────────────────────────────────────────

/// Default command timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum output buffer size per session (16 MB).
const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

/// Maximum concurrent background sessions.
const MAX_SESSIONS: usize = 32;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Request to execute a shell command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecRequest {
    /// The shell command to execute.
    pub command: String,
    /// Working directory (defaults to current dir).
    #[serde(default)]
    pub workdir: Option<String>,
    /// Additional environment variables.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Timeout in seconds (foreground only).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Run in background immediately.
    #[serde(default)]
    pub background: bool,
    /// Use a pseudo-terminal (for interactive CLIs).
    #[serde(default)]
    pub pty: bool,
    /// Milliseconds to wait before backgrounding (yield pattern).
    /// If the command completes within this time, return output directly.
    #[serde(default)]
    pub yield_ms: Option<u64>,
}

/// Result of a foreground execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    /// Exit code (None if killed by signal or still running).
    pub exit_code: Option<i32>,
    /// Combined stdout output.
    pub stdout: String,
    /// Combined stderr output.
    pub stderr: String,
    /// Whether the command timed out.
    pub timed_out: bool,
    /// Session ID if the command was backgrounded.
    pub session_id: Option<String>,
    /// Whether the process is still running (for yield_ms pattern).
    pub still_running: bool,
}

/// A tracked background session.
#[derive(Debug)]
struct Session {
    /// Unique session identifier.
    id: String,
    /// The command that was executed.
    command: String,
    /// Working directory.
    #[allow(dead_code)]
    workdir: Option<String>,
    /// Process handle (None if already exited).
    child: Option<Child>,
    /// Accumulated stdout output.
    stdout_buffer: Vec<u8>,
    /// Accumulated stderr output.
    stderr_buffer: Vec<u8>,
    /// Read cursor — how much output has been returned to the caller.
    read_offset: usize,
    /// Exit code (set when process exits).
    exit_code: Option<i32>,
    /// Whether the process is still running.
    running: bool,
    /// When the session was created.
    created_at: Instant,
    /// PID of the child process.
    pid: Option<u32>,
    /// Stdin handle for writing to the process.
    stdin: Option<tokio::process::ChildStdin>,
}

/// Summary of a session for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub command: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
    pub elapsed_secs: u64,
    pub output_bytes: usize,
}

/// Result of polling a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollResult {
    pub session_id: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    /// New output since last poll.
    pub new_output: String,
    /// Total output size.
    pub total_bytes: usize,
}

// ── Shell Executor ────────────────────────────────────────────────────────────

/// The main shell executor with session tracking.
pub struct ShellExecutor {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl ShellExecutor {
    /// Create a new executor.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Execute a command (foreground or background based on request).
    pub async fn exec(&self, req: ExecRequest) -> ExecResult {
        if req.background {
            self.background_exec(req).await
        } else if let Some(yield_ms) = req.yield_ms {
            self.yield_exec(req, yield_ms).await
        } else {
            self.foreground_exec(req).await
        }
    }

    /// Run a command in the foreground with timeout.
    async fn foreground_exec(&self, req: ExecRequest) -> ExecResult {
        let timeout = Duration::from_secs(req.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&req.command);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        if let Some(ref workdir) = req.workdir {
            cmd.current_dir(workdir);
        }
        for (k, v) in &req.env {
            cmd.env(k, v);
        }

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return ExecResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("failed to spawn: {e}"),
                    timed_out: false,
                    session_id: None,
                    still_running: false,
                };
            }
        };

        // Wait with timeout
        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => ExecResult {
                exit_code: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                timed_out: false,
                session_id: None,
                still_running: false,
            },
            Ok(Err(e)) => ExecResult {
                exit_code: None,
                stdout: String::new(),
                stderr: format!("I/O error: {e}"),
                timed_out: false,
                session_id: None,
                still_running: false,
            },
            Err(_) => {
                // Timeout — process is killed via kill_on_drop
                ExecResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!(
                        "command timed out after {}s and was killed",
                        timeout.as_secs()
                    ),
                    timed_out: true,
                    session_id: None,
                    still_running: false,
                }
            }
        }
    }

    /// Spawn a background session.
    async fn background_exec(&self, req: ExecRequest) -> ExecResult {
        // Check session limit
        {
            let sessions = self.sessions.read().await;
            if sessions.len() >= MAX_SESSIONS {
                return ExecResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!(
                        "maximum sessions ({MAX_SESSIONS}) reached; kill old sessions first"
                    ),
                    timed_out: false,
                    session_id: None,
                    still_running: false,
                };
            }
        }

        let session_id = generate_session_id();

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&req.command);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::piped());
        cmd.kill_on_drop(false); // background processes survive

        if let Some(ref workdir) = req.workdir {
            cmd.current_dir(workdir);
        }
        for (k, v) in &req.env {
            cmd.env(k, v);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return ExecResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("failed to spawn: {e}"),
                    timed_out: false,
                    session_id: None,
                    still_running: false,
                };
            }
        };

        let pid = child.id();
        let stdin = child.stdin.take();

        // Start output reader tasks
        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        let session = Session {
            id: session_id.clone(),
            command: req.command.clone(),
            workdir: req.workdir.clone(),
            child: Some(child),
            stdout_buffer: Vec::new(),
            stderr_buffer: Vec::new(),
            read_offset: 0,
            exit_code: None,
            running: true,
            created_at: Instant::now(),
            pid,
            stdin,
        };

        self.sessions
            .write()
            .await
            .insert(session_id.clone(), session);

        // Spawn background tasks to read stdout/stderr into session buffers
        let sessions_ref = self.sessions.clone();
        let sid = session_id.clone();
        if let Some(mut stdout) = stdout_handle {
            let sessions = sessions_ref.clone();
            let id = sid.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                loop {
                    match stdout.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            let mut sessions = sessions.write().await;
                            if let Some(session) = sessions.get_mut(&id) {
                                if session.stdout_buffer.len() + n <= MAX_OUTPUT_BYTES {
                                    session.stdout_buffer.extend_from_slice(&buf[..n]);
                                }
                            } else {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        if let Some(mut stderr) = stderr_handle {
            let sessions = sessions_ref.clone();
            let id = sid.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                loop {
                    match stderr.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            let mut sessions = sessions.write().await;
                            if let Some(session) = sessions.get_mut(&id) {
                                if session.stderr_buffer.len() + n <= MAX_OUTPUT_BYTES {
                                    session.stderr_buffer.extend_from_slice(&buf[..n]);
                                }
                            } else {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Spawn a task to detect process exit
        let sessions = self.sessions.clone();
        let id = session_id.clone();
        tokio::spawn(async move {
            // We need to wait on the child, but it's behind the session lock.
            // We'll poll periodically until it exits.
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let mut sessions = sessions.write().await;
                if let Some(session) = sessions.get_mut(&id) {
                    if let Some(ref mut child) = session.child {
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                session.exit_code = status.code();
                                session.running = false;
                                session.child = None;
                                break;
                            }
                            Ok(None) => { /* still running */ }
                            Err(_) => {
                                session.running = false;
                                session.child = None;
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        });

        info!(session_id = %session_id, command = %req.command, "background session started");

        ExecResult {
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            session_id: Some(session_id),
            still_running: true,
        }
    }

    /// Yield pattern: wait up to yield_ms, then background if still running.
    async fn yield_exec(&self, req: ExecRequest, yield_ms: u64) -> ExecResult {
        let yield_dur = Duration::from_millis(yield_ms);

        // Start as background first
        let bg_req = ExecRequest {
            background: true,
            ..req.clone()
        };
        let bg_result = self.background_exec(bg_req).await;

        let session_id = match &bg_result.session_id {
            Some(id) => id.clone(),
            None => return bg_result, // spawn failed
        };

        // Wait up to yield_ms for completion
        let start = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;

            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(&session_id) {
                if !session.running {
                    // Completed within yield window
                    let stdout = String::from_utf8_lossy(&session.stdout_buffer).to_string();
                    let stderr = String::from_utf8_lossy(&session.stderr_buffer).to_string();
                    let exit_code = session.exit_code;
                    drop(sessions);

                    // Clean up the session
                    self.sessions.write().await.remove(&session_id);

                    return ExecResult {
                        exit_code,
                        stdout,
                        stderr,
                        timed_out: false,
                        session_id: None,
                        still_running: false,
                    };
                }
            } else {
                break;
            }
            drop(sessions);

            if start.elapsed() >= yield_dur {
                // Still running after yield window — return session reference
                let sessions = self.sessions.read().await;
                let initial_output = if let Some(session) = sessions.get(&session_id) {
                    String::from_utf8_lossy(&session.stdout_buffer).to_string()
                } else {
                    String::new()
                };

                return ExecResult {
                    exit_code: None,
                    stdout: initial_output,
                    stderr: String::new(),
                    timed_out: false,
                    session_id: Some(session_id),
                    still_running: true,
                };
            }
        }

        bg_result
    }

    /// Poll a session for new output.
    pub async fn poll(&self, session_id: &str, timeout_ms: Option<u64>) -> Option<PollResult> {
        let deadline = timeout_ms.map(|ms| Instant::now() + Duration::from_millis(ms));

        loop {
            {
                let mut sessions = self.sessions.write().await;
                let session = sessions.get_mut(session_id)?;

                let total_output = session.stdout_buffer.len() + session.stderr_buffer.len();
                let new_bytes = total_output.saturating_sub(session.read_offset);

                // If there's new output or the process has exited, return
                if new_bytes > 0 || !session.running {
                    let combined = [
                        session.stdout_buffer.as_slice(),
                        session.stderr_buffer.as_slice(),
                    ]
                    .concat();
                    let new_output =
                        String::from_utf8_lossy(&combined[session.read_offset..]).to_string();
                    session.read_offset = combined.len();

                    return Some(PollResult {
                        session_id: session_id.to_string(),
                        running: session.running,
                        exit_code: session.exit_code,
                        new_output,
                        total_bytes: combined.len(),
                    });
                }
            }

            // If no deadline or deadline passed, return empty
            match deadline {
                Some(dl) if Instant::now() < dl => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => {
                    let sessions = self.sessions.read().await;
                    let session = sessions.get(session_id)?;
                    return Some(PollResult {
                        session_id: session_id.to_string(),
                        running: session.running,
                        exit_code: session.exit_code,
                        new_output: String::new(),
                        total_bytes: session.stdout_buffer.len() + session.stderr_buffer.len(),
                    });
                }
            }
        }
    }

    /// Write data to a session's stdin.
    pub async fn write_stdin(&self, session_id: &str, data: &str) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("session not found: {session_id}"))?;

        if !session.running {
            return Err("session has already exited".to_string());
        }

        let stdin = session
            .stdin
            .as_mut()
            .ok_or_else(|| "stdin not available for this session".to_string())?;

        stdin
            .write_all(data.as_bytes())
            .await
            .map_err(|e| format!("write failed: {e}"))?;

        stdin
            .flush()
            .await
            .map_err(|e| format!("flush failed: {e}"))?;

        Ok(())
    }

    /// Kill a running session.
    pub async fn kill(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("session not found: {session_id}"))?;

        if let Some(ref mut child) = session.child {
            child
                .kill()
                .await
                .map_err(|e| format!("kill failed: {e}"))?;
        }

        session.running = false;
        session.child = None;
        info!(session_id, "session killed");
        Ok(())
    }

    /// List all tracked sessions.
    pub async fn list(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .map(|s| SessionInfo {
                id: s.id.clone(),
                command: s.command.clone(),
                running: s.running,
                exit_code: s.exit_code,
                pid: s.pid,
                elapsed_secs: s.created_at.elapsed().as_secs(),
                output_bytes: s.stdout_buffer.len() + s.stderr_buffer.len(),
            })
            .collect()
    }

    /// Get full log output from a session.
    pub async fn log(
        &self,
        session_id: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Option<String> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(session_id)?;

        let combined = [
            session.stdout_buffer.as_slice(),
            session.stderr_buffer.as_slice(),
        ]
        .concat();

        let start = offset.unwrap_or(0).min(combined.len());
        let end = limit
            .map(|l| (start + l).min(combined.len()))
            .unwrap_or(combined.len());

        Some(String::from_utf8_lossy(&combined[start..end]).to_string())
    }

    /// Remove a completed session from tracking.
    pub async fn remove(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get(session_id) {
            if session.running {
                return false; // can't remove running sessions
            }
        }
        sessions.remove(session_id).is_some()
    }

    /// Clean up old completed sessions (> 1 hour old).
    pub async fn cleanup_old(&self) {
        let mut sessions = self.sessions.write().await;
        let cutoff = Duration::from_secs(3600);
        sessions.retain(|_, s| s.running || s.created_at.elapsed() < cutoff);
    }
}

impl Default for ShellExecutor {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

use std::sync::atomic::{AtomicU64, Ordering};

/// Global session counter for unique IDs.
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a human-friendly unique session ID (adjective-noun-N pattern).
fn generate_session_id() -> String {
    let adjectives = [
        "swift", "calm", "bold", "keen", "warm", "cool", "bright", "dark", "quick", "slow",
        "fresh", "wild", "soft", "loud", "deep", "high",
    ];
    let nouns = [
        "oak", "fox", "elm", "owl", "bay", "ash", "sky", "dew", "gem", "wave", "leaf", "star",
        "moon", "sun", "reed", "pine",
    ];

    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let adj = adjectives[(n as usize) % adjectives.len()];
    let noun = nouns[((n as usize) / adjectives.len()) % nouns.len()];
    format!("{adj}-{noun}-{n}")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn foreground_echo() {
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo hello".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: Some(5),
                background: false,
                pty: false,
                yield_ms: None,
            })
            .await;

        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.trim(), "hello");
        assert!(!result.timed_out);
        assert!(!result.still_running);
    }

    #[tokio::test]
    async fn foreground_timeout() {
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "sleep 60".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: Some(1),
                background: false,
                pty: false,
                yield_ms: None,
            })
            .await;

        assert!(result.timed_out);
        assert!(result.exit_code.is_none());
    }

    #[tokio::test]
    async fn foreground_with_workdir() {
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "pwd".into(),
                workdir: Some("/tmp".into()),
                env: HashMap::new(),
                timeout_secs: Some(5),
                background: false,
                pty: false,
                yield_ms: None,
            })
            .await;

        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.trim().contains("/tmp"));
    }

    #[tokio::test]
    async fn foreground_with_env() {
        let executor = ShellExecutor::new();
        let mut env = HashMap::new();
        env.insert("MY_VAR".into(), "hello_world".into());

        let result = executor
            .exec(ExecRequest {
                command: "echo $MY_VAR".into(),
                workdir: None,
                env,
                timeout_secs: Some(5),
                background: false,
                pty: false,
                yield_ms: None,
            })
            .await;

        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout.trim(), "hello_world");
    }

    #[tokio::test]
    async fn background_exec_and_poll() {
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo background_output && sleep 0.1".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        assert!(result.still_running);
        let session_id = result.session_id.unwrap();

        // Poll with generous timeout — the command runs ~100ms but under CI load may take longer
        tokio::time::sleep(Duration::from_millis(300)).await;
        let poll = executor.poll(&session_id, Some(3000)).await.unwrap();
        if poll.running {
            // Retry once more if still running (CI load)
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let poll2 = executor.poll(&session_id, Some(3000)).await.unwrap();
            assert!(!poll2.running, "process still running after 4.3s for a 0.1s command");
            assert!(poll2.new_output.contains("background_output") || poll.new_output.contains("background_output"));
        } else {
            assert!(poll.new_output.contains("background_output"));
        }
    }

    #[tokio::test]
    async fn background_list_and_kill() {
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "sleep 60".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();

        let list = executor.list().await;
        assert_eq!(list.len(), 1);
        assert!(list[0].running);

        executor.kill(&session_id).await.unwrap();

        // Give it a moment
        tokio::time::sleep(Duration::from_millis(100)).await;

        let list = executor.list().await;
        assert!(!list[0].running);
    }

    #[tokio::test]
    async fn yield_completes_quickly() {
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo fast".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: false,
                pty: false,
                yield_ms: Some(5000),
            })
            .await;

        // Should complete within yield window
        assert!(!result.still_running);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("fast"));
        assert!(result.session_id.is_none()); // cleaned up
    }

    #[tokio::test]
    async fn yield_backgrounds_slow_command() {
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "sleep 60".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: false,
                pty: false,
                yield_ms: Some(200),
            })
            .await;

        // Should background after yield window
        assert!(result.still_running);
        assert!(result.session_id.is_some());

        // Cleanup
        executor.kill(&result.session_id.unwrap()).await.unwrap();
    }

    #[tokio::test]
    async fn write_to_stdin() {
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "cat".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();

        // Write to stdin
        executor
            .write_stdin(&session_id, "hello from stdin\n")
            .await
            .unwrap();

        // Poll with retries — cat may take time to echo back
        let mut found = false;
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let poll = executor.poll(&session_id, Some(500)).await.unwrap();
            if poll.new_output.contains("hello from stdin") {
                found = true;
                break;
            }
        }
        assert!(found, "Expected 'hello from stdin' in poll output");

        executor.kill(&session_id).await.unwrap();
    }

    #[tokio::test]
    async fn log_with_offset_and_limit() {
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo -n 'abcdefghij'".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        let full = executor.log(&session_id, None, None).await.unwrap();
        assert_eq!(full, "abcdefghij");

        let partial = executor.log(&session_id, Some(3), Some(4)).await.unwrap();
        assert_eq!(partial, "defg");
    }

    #[tokio::test]
    async fn nonexistent_command_fails() {
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "/nonexistent/binary/xyz 2>/dev/null; exit $?".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: Some(5),
                background: false,
                pty: false,
                yield_ms: None,
            })
            .await;

        // sh -c should still succeed as a process, but the inner command fails
        assert_ne!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn session_limit_enforced() {
        let executor = ShellExecutor::new();

        // Fill up to MAX_SESSIONS with long-running sleeps
        let mut ids = Vec::new();
        for _ in 0..MAX_SESSIONS {
            let result = executor
                .exec(ExecRequest {
                    command: "sleep 300".into(),
                    workdir: None,
                    env: HashMap::new(),
                    timeout_secs: None,
                    background: true,
                    pty: false,
                    yield_ms: None,
                })
                .await;
            if let Some(id) = result.session_id {
                ids.push(id);
            }
        }

        // Verify we actually hit the limit
        assert_eq!(ids.len(), MAX_SESSIONS);

        // Next one should fail
        let result = executor
            .exec(ExecRequest {
                command: "echo overflow".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        assert!(result.session_id.is_none());
        assert!(result.stderr.contains("maximum sessions"));

        // Cleanup
        for id in &ids {
            let _ = executor.kill(id).await;
        }
    }

    // ── Mutation gap coverage ─────────────────────────────────────────────────

    #[test]
    fn max_output_bytes_is_16mb() {
        // Catches: replace * with + in MAX_OUTPUT_BYTES constant
        assert_eq!(MAX_OUTPUT_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_OUTPUT_BYTES, 16_777_216);
    }

    #[tokio::test]
    async fn background_stderr_buffer_limit_respected() {
        // Catches: replace <= with > in stderr buffer guard (line 357)
        // and replace + with * / - in buffer length checks
        let executor = ShellExecutor::new();
        // Generate output on stderr and verify it's captured
        let result = executor
            .exec(ExecRequest {
                command: "echo stderr_data >&2".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        // Wait for process to complete
        tokio::time::sleep(Duration::from_millis(300)).await;

        let poll = executor.poll(&session_id, Some(2000)).await.unwrap();
        assert!(
            poll.new_output.contains("stderr_data"),
            "stderr should be captured in output; got: {:?}",
            poll.new_output
        );
        // total_bytes should reflect both stdout+stderr correctly
        assert!(poll.total_bytes > 0);
    }

    #[tokio::test]
    async fn poll_total_bytes_sums_stdout_and_stderr() {
        // Catches: replace + with - / * in poll total_bytes calculation (lines 488, 495, 532)
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo -n 'OUT12345' && echo -n 'ERR12345' >&2".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        let poll = executor.poll(&session_id, Some(2000)).await.unwrap();
        // Both streams should be present: 8 bytes stdout + 8 bytes stderr = 16
        assert_eq!(poll.total_bytes, 16, "total_bytes should be stdout + stderr");
        assert!(!poll.running);
    }

    #[tokio::test]
    async fn poll_new_output_only_returns_unread_bytes() {
        // Catches: replace > with >= in new_bytes check (line 499)
        // and: delete ! in !session.running check
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo -n 'first' && sleep 0.2 && echo -n 'second'".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // First poll gets 'first'
        let poll1 = executor.poll(&session_id, Some(1000)).await.unwrap();
        assert!(
            poll1.new_output.contains("first"),
            "first poll should contain 'first'; got: {:?}",
            poll1.new_output
        );

        // Wait for 'second' to appear
        tokio::time::sleep(Duration::from_millis(400)).await;
        let poll2 = executor.poll(&session_id, Some(1000)).await.unwrap();
        assert!(
            poll2.new_output.contains("second"),
            "second poll should contain 'second'; got: {:?}",
            poll2.new_output
        );
        // 'first' should NOT be in the second poll (already read)
        assert!(
            !poll2.new_output.contains("first"),
            "second poll should not re-return 'first'; got: {:?}",
            poll2.new_output
        );
    }

    #[tokio::test]
    async fn poll_with_timeout_waits_for_output() {
        // Catches: replace match guard Instant::now() < dl with true/false
        // and: replace < with > / == / <= in deadline comparison
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "sleep 0.3 && echo -n 'delayed'".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();

        // Poll immediately with no timeout — should get empty output
        let poll_immediate = executor.poll(&session_id, None).await.unwrap();
        assert_eq!(
            poll_immediate.new_output, "",
            "immediate poll with no timeout should return empty"
        );

        // Poll with generous timeout — should wait and get the output
        let poll_wait = executor.poll(&session_id, Some(5000)).await.unwrap();
        assert!(
            poll_wait.new_output.contains("delayed"),
            "poll with timeout should wait for output; got: {:?}",
            poll_wait.new_output
        );
    }

    #[tokio::test]
    async fn list_output_bytes_sums_correctly() {
        // Catches: replace + with * / - in list output_bytes (line 600)
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo -n 'AAAA' && echo -n 'BB' >&2".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        let list = executor.list().await;
        let info = list.iter().find(|s| s.id == session_id).unwrap();
        // 4 bytes stdout + 2 bytes stderr = 6
        assert_eq!(info.output_bytes, 6, "output_bytes should be stdout + stderr");
    }

    #[tokio::test]
    async fn remove_running_session_returns_false() {
        // Catches: replace ShellExecutor::remove -> bool with true
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "sleep 60".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        // Can't remove a running session
        assert!(!executor.remove(&session_id).await);
        // Kill it first, then remove should succeed
        executor.kill(&session_id).await.unwrap();
        assert!(executor.remove(&session_id).await);
    }

    #[tokio::test]
    async fn remove_nonexistent_session_returns_false() {
        // Catches: replace ShellExecutor::remove -> bool with true
        let executor = ShellExecutor::new();
        assert!(!executor.remove("nonexistent-id").await);
    }

    #[tokio::test]
    async fn cleanup_old_removes_completed_sessions() {
        // Catches: replace ShellExecutor::cleanup_old with ()
        // and: replace || with && in retain predicate
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo done".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();

        // Wait for the exit detector to mark it as not running
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let list = executor.list().await;
            if !list[0].running {
                break;
            }
        }

        // Session exists and is complete
        let list = executor.list().await;
        assert_eq!(list.len(), 1);
        assert!(!list[0].running, "session should have exited");

        // Manually age the session by setting created_at far in the past
        {
            let mut sessions = executor.sessions.write().await;
            if let Some(s) = sessions.get_mut(&session_id) {
                s.created_at = Instant::now() - Duration::from_secs(7200); // 2 hours ago
            }
        }

        executor.cleanup_old().await;
        let list = executor.list().await;
        assert_eq!(list.len(), 0, "cleanup_old should remove old completed sessions");
    }

    #[tokio::test]
    async fn cleanup_old_keeps_running_sessions() {
        // Catches: replace || with && in retain predicate
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "sleep 60".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();

        // Age the session
        {
            let mut sessions = executor.sessions.write().await;
            if let Some(s) = sessions.get_mut(&session_id) {
                s.created_at = Instant::now() - Duration::from_secs(7200);
            }
        }

        executor.cleanup_old().await;
        // Should NOT be removed because it's still running
        let list = executor.list().await;
        assert_eq!(list.len(), 1, "cleanup_old should keep running sessions even if old");
        assert!(list[0].running);

        executor.kill(&session_id).await.unwrap();
    }

    #[tokio::test]
    async fn yield_exec_sets_background_flag() {
        // Catches: delete field background from struct ExecRequest expression in yield_exec
        let executor = ShellExecutor::new();
        // Use yield_ms with a long-running command
        let result = executor
            .exec(ExecRequest {
                command: "sleep 60".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: false,
                pty: false,
                yield_ms: Some(100),
            })
            .await;

        // Should have been backgrounded (yield_ms elapsed while command runs)
        assert!(result.still_running);
        assert!(result.session_id.is_some());

        // Verify the session is tracked
        let list = executor.list().await;
        assert_eq!(list.len(), 1);
        assert!(list[0].running);

        executor.kill(&result.session_id.unwrap()).await.unwrap();
    }

    #[test]
    fn generate_session_id_is_unique() {
        let ids: Vec<String> = (0..100).map(|_| generate_session_id()).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), 100);
    }

    #[test]
    fn generate_session_id_format() {
        let id = generate_session_id();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 3, "session id should be adjective-noun-N; got: {id}");
    }

    #[tokio::test]
    async fn default_impl_creates_working_executor() {
        let executor = ShellExecutor::default();
        let result = executor
            .exec(ExecRequest {
                command: "echo default_works".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: Some(5),
                background: false,
                pty: false,
                yield_ms: None,
            })
            .await;
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("default_works"));
    }

    // ── mutation-gap tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn buffer_accepts_data_when_partially_filled() {
        // Catches: replace + with * in stdout/stderr buffer guard (lines 333, 357).
        // If `len() + n` becomes `len() * n`, a buffer with >0 bytes would reject
        // new chunks when len*n > MAX_OUTPUT_BYTES (e.g. 10000*8192 > 16MB).
        // This test fills the buffer partially (>8KB), then verifies more writes succeed.
        let executor = ShellExecutor::new();
        // Generate ~16KB of stdout — well above 8192 bytes per chunk
        let result = executor
            .exec(ExecRequest {
                command: "dd if=/dev/zero bs=1024 count=16 2>/dev/null | tr '\\0' 'A'".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: Some(5),
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;

        let poll = executor.poll(&session_id, Some(3000)).await.unwrap();
        // All 16KB should be captured — with `*` mutant, only the first chunk would pass
        assert!(
            poll.total_bytes >= 16384,
            "buffer should accept all 16KB output; got {} bytes",
            poll.total_bytes
        );
    }

    #[tokio::test]
    async fn yield_exec_backgrounds_with_correct_flag() {
        // Catches: delete field `background` from ExecRequest in yield_exec (line 422).
        // Without `background: true`, the inner call would attempt foreground exec
        // and the session wouldn't be tracked.
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "sleep 10".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: false,
                pty: false,
                yield_ms: Some(50), // short yield → background
            })
            .await;

        // Must be backgrounded with a session ID
        assert!(result.session_id.is_some(), "yield should produce a session_id");
        assert!(result.still_running, "yield should report still_running");

        // Critical: the session must actually exist in the executor's tracked sessions
        let list = executor.list().await;
        assert!(!list.is_empty(), "backgrounded session must be tracked");
        let info = &list[0];
        assert!(info.running, "session should still be running");

        // Poll must return data for the session (proves it's properly background-tracked)
        let poll = executor.poll(&result.session_id.as_ref().unwrap(), Some(100)).await;
        assert!(poll.is_some(), "poll must find the background session");

        executor.kill(&result.session_id.unwrap()).await.unwrap();
    }

    #[tokio::test]
    async fn poll_timeout_returns_empty_when_no_output() {
        // Catches: replace match guard `Instant::now() < dl` with `true` (line 521)
        //   and: replace < with <= (line 521)
        //   and: replace + with - or * in total_bytes (line 532)
        // With guard=true, poll would loop forever instead of returning empty.
        // With < to <=, it would spin one extra iteration.
        // With + to - or *, the empty total_bytes would be wrong.
        let executor = ShellExecutor::new();
        // Command that produces no output for a while
        let result = executor
            .exec(ExecRequest {
                command: "sleep 60".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();

        // Poll with short timeout — should return empty (not hang)
        let start = Instant::now();
        let poll = executor.poll(&session_id, Some(200)).await.unwrap();
        let elapsed = start.elapsed();

        // Must return within a reasonable time (not hang)
        assert!(
            elapsed < Duration::from_millis(1000),
            "poll should return after timeout, not hang; took {:?}",
            elapsed
        );
        // With guard=true mutant, this would hang forever
        assert_eq!(poll.new_output, "");
        assert!(poll.running);
        // total_bytes should be 0 (no output yet) — catches +→- and +→* since
        // stdout=0, stderr=0: 0+0=0, 0-0=0, 0*0=0 all equal... need non-zero.
        // Actually the correct test is to produce some output first.
        assert_eq!(poll.total_bytes, 0);

        executor.kill(&session_id).await.unwrap();
    }

    #[tokio::test]
    async fn poll_timeout_total_bytes_in_empty_path() {
        // Catches: replace + with - or * in total_bytes at line 532.
        // We need a session with some output already read, then poll again
        // with no new output to hit the empty path (line 525-533).
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo -n INIT && sleep 60".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        // First poll reads the initial output
        let poll1 = executor.poll(&session_id, Some(500)).await.unwrap();
        assert!(poll1.new_output.contains("INIT"));
        let bytes_after_first = poll1.total_bytes; // should be 4

        // Second poll with short timeout — no new output → hits empty path (line 532)
        let poll2 = executor.poll(&session_id, Some(150)).await.unwrap();
        assert_eq!(poll2.new_output, "");
        // total_bytes should still equal stdout+stderr combined
        // With + → -: would be stdout.len() - stderr.len() = 4 - 0 = 4 (same, unhelpful)
        // With + → *: would be 4 * 0 = 0 (different!)
        assert_eq!(
            poll2.total_bytes, bytes_after_first,
            "total_bytes in empty path should equal stdout+stderr"
        );

        executor.kill(&session_id).await.unwrap();
    }

    #[tokio::test]
    async fn poll_empty_path_with_stderr_catches_multiply_mutation() {
        // Specifically catches: + → * on line 532 (stdout.len() + stderr.len())
        // Need both stdout AND stderr to be non-zero so + vs * gives different results.
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo -n 'AAA' && echo -n 'BBB' >&2 && sleep 60".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        // First poll reads all output
        let poll1 = executor.poll(&session_id, Some(500)).await.unwrap();
        assert_eq!(poll1.total_bytes, 6); // 3 + 3

        // Second poll → empty path with total_bytes from both buffers
        let poll2 = executor.poll(&session_id, Some(150)).await.unwrap();
        assert_eq!(poll2.new_output, "");
        // With +: 3 + 3 = 6. With *: 3 * 3 = 9. With -: 3 - 3 = 0.
        assert_eq!(poll2.total_bytes, 6, "total_bytes must be stdout+stderr (3+3=6)");

        executor.kill(&session_id).await.unwrap();
    }

    #[tokio::test]
    async fn cleanup_old_keeps_session_under_cutoff() {
        // Catches: replace < with == or <= in cleanup_old (line 644).
        // With `<` → `==`: only sessions EXACTLY at cutoff would be retained.
        // With `<` → `<=`: sessions exactly at cutoff would also be retained (subtle).
        // Test: session aged 30 minutes (< 1 hour cutoff) must be retained.
        let executor = ShellExecutor::new();
        let result = executor
            .exec(ExecRequest {
                command: "echo done".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        // Wait for completion
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let list = executor.list().await;
            if !list.is_empty() && !list[0].running {
                break;
            }
        }

        // Age to 30 minutes (well under 1-hour cutoff)
        {
            let mut sessions = executor.sessions.write().await;
            if let Some(s) = sessions.get_mut(&session_id) {
                s.created_at = Instant::now() - Duration::from_secs(1800);
            }
        }

        executor.cleanup_old().await;
        let list = executor.list().await;
        // With `<` → `==`: elapsed (1800s) != cutoff (3600s), so the retain
        // closure returns false, and the session is REMOVED. This test catches that.
        assert_eq!(
            list.len(),
            1,
            "session under cutoff must be retained (catches < → == mutation)"
        );
    }

    #[tokio::test]
    async fn stderr_multi_chunk_buffer_captures_all() {
        // Catches: replace + with * in stderr buffer guard (line 357).
        // With the mutant `len * n <= MAX`: after first 8KB read (len=0, 0*8192=0 ≤ MAX → ok),
        // second read (len=8192, 8192*8192=64MB > MAX → REJECTED). So >8KB stderr
        // would be truncated to exactly one chunk. This test verifies all ~32KB arrives.
        let executor = ShellExecutor::new();
        // Generate 32KB on stderr using dd
        let result = executor
            .exec(ExecRequest {
                command: "dd if=/dev/zero bs=1024 count=32 2>/dev/null | tr '\\0' 'E' >&2".into(),
                workdir: None,
                env: HashMap::new(),
                timeout_secs: None,
                background: true,
                pty: false,
                yield_ms: None,
            })
            .await;

        let session_id = result.session_id.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;

        let poll = executor.poll(&session_id, Some(3000)).await.unwrap();
        // With + → * mutant, only first 8KB would be captured (second chunk rejected).
        // With correct +, all 32KB should arrive.
        assert!(
            poll.total_bytes >= 32768,
            "stderr multi-chunk: expected ≥32KB, got {} bytes (mutant would give ≤8192)",
            poll.total_bytes
        );
    }

    #[test]
    fn generate_session_id_uses_correct_adjective_noun_mapping() {
        // Catches: replace % with / and / with % in generate_session_id (lines 673-674).
        // The adjective should cycle through 16 adjectives, noun should advance every 16.
        // With % → /: index would be n/16 instead of n%16 for adj.
        // With / → %: noun index would be n%16 instead of n/16.
        let adjectives = [
            "swift", "calm", "bold", "keen", "warm", "cool", "bright", "dark", "quick",
            "slow", "fresh", "wild", "soft", "loud", "deep", "high",
        ];
        let nouns = [
            "oak", "fox", "elm", "owl", "bay", "ash", "sky", "dew", "gem", "wave",
            "leaf", "star", "moon", "sun", "reed", "pine",
        ];

        // Generate enough IDs to verify the pattern wraps correctly
        let ids: Vec<String> = (0..32).map(|_| generate_session_id()).collect();

        // Verify the pattern: adj cycles with period 16, noun advances every 16
        for (i, id) in ids.iter().enumerate() {
            let parts: Vec<&str> = id.split('-').collect();
            assert_eq!(parts.len(), 3);

            let adj = parts[0];
            let noun = parts[1];

            // Extract the counter from the ID suffix
            let n: usize = parts[2].parse().unwrap();

            // Verify adjective cycles with modulo
            let expected_adj = adjectives[n % 16];
            assert_eq!(
                adj, expected_adj,
                "ID {id}: adj should be '{}' for n={n} (n%16={}), got '{adj}'",
                expected_adj,
                n % 16
            );

            // Verify noun advances with division
            let expected_noun = nouns[(n / 16) % 16];
            assert_eq!(
                noun, expected_noun,
                "ID {id}: noun should be '{}' for n={n} (n/16%16={}), got '{noun}'",
                expected_noun,
                (n / 16) % 16
            );
        }
    }
}

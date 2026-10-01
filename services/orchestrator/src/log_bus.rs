use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrchestratorLogEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub level: String,  // "INFO", "WARN", "ERROR", "DEBUG", "SUCCESS"
    pub source: String, // "ai_router", "generation", "validation", "execution", "executor", "policy", "store", "engine", etc.
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// The three sanitisation patterns, compiled once.
///
/// These were previously `Regex::new(...)` calls *inside*
/// `sanitize_log_message`, i.e. recompiled on every single log line. Regex
/// compilation is one of the more expensive operations in Rust, and
/// `LogBus::emit` calls this function once per emitted entry — so a run
/// emitting thousands of log lines paid three compiles per line for patterns
/// that can never change at runtime. `LazyLock` (stable since Rust 1.80) compiles
/// each one on first use and reuses it after that, with identical behaviour.
static AIQA_KEY_PATTERN: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"aiqa_[a-zA-Z0-9_-]{12,}").expect("static pattern"));
static BEARER_PATTERN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?i)bearer\s+[a-zA-Z0-9_\-\.]{12,}").expect("static pattern")
});
static SECRET_PARAM_PATTERN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?i)(password|secret|api_key|token|auth_token)=([^&\s]+)")
        .expect("static pattern")
});

/// Sanitizes log messages to ensure sensitive tokens, API keys, and credentials
/// are never exposed to the frontend log stream.
///
/// Compiled patterns live in the three `LazyLock` statics above; this function
/// is called once per log entry, so pattern compilation must not happen here.
pub fn sanitize_log_message(msg: &str) -> String {
    // 1. Mask AIQA bearer / API keys (e.g. aiqa_0123456789abcdef...)
    let cleaned = AIQA_KEY_PATTERN.replace_all(msg, "aiqa_***");
    // 2. Mask Authorization Bearer tokens
    let cleaned = BEARER_PATTERN.replace_all(&cleaned, "Bearer ***");
    // 3. Mask parameters with passwords, keys, or secrets
    let cleaned = SECRET_PARAM_PATTERN.replace_all(&cleaned, "$1=***");
    cleaned.into_owned()
}

pub struct LogBus {
    capacity: usize,
    buffer: Arc<RwLock<VecDeque<OrchestratorLogEntry>>>,
    tx: broadcast::Sender<OrchestratorLogEntry>,
}

impl LogBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self {
            capacity,
            buffer: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
            tx,
        }
    }

    /// Emits a structured log entry, sanitizes sensitive content, appends to the
    /// bounded in-memory buffer, and broadcasts to active live stream subscribers.
    pub async fn emit(
        &self,
        level: &str,
        source: &str,
        message: &str,
        run_id: Option<&str>,
        project_id: Option<&str>,
        task_id: Option<&str>,
    ) -> OrchestratorLogEntry {
        let entry = OrchestratorLogEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            level: level.to_uppercase(),
            source: source.to_string(),
            message: sanitize_log_message(message),
            run_id: run_id.map(ToString::to_string),
            project_id: project_id.map(ToString::to_string),
            task_id: task_id.map(ToString::to_string),
        };

        {
            let mut buf = self.buffer.write().await;
            if buf.len() >= self.capacity {
                buf.pop_front();
            }
            buf.push_back(entry.clone());
        }

        let _ = self.tx.send(entry.clone());
        entry
    }

    /// Retrieves the most recent in-memory log history, optionally filtered by project.
    /// Global logs (where project_id is None) are included alongside project-specific logs.
    pub async fn recent_logs(
        &self,
        project_id: Option<&str>,
        limit: usize,
    ) -> Vec<OrchestratorLogEntry> {
        let buf = self.buffer.read().await;
        let iter = buf
            .iter()
            .filter(|entry| match (project_id, &entry.project_id) {
                (Some(pid), Some(entry_pid)) => pid == entry_pid,
                (Some(_), None) => true, // Global logs are visible to project streams
                (None, _) => true,
            });

        let matched: Vec<OrchestratorLogEntry> = iter.cloned().collect();
        if matched.len() > limit {
            matched[matched.len() - limit..].to_vec()
        } else {
            matched
        }
    }

    /// Subscribes to the live broadcast channel for new log events.
    pub fn subscribe(&self) -> broadcast::Receiver<OrchestratorLogEntry> {
        self.tx.subscribe()
    }
}

impl Default for LogBus {
    fn default() -> Self {
        Self::new(300)
    }
}

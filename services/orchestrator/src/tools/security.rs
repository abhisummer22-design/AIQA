//! Security and guardrail utilities for AgentTools.
//!
//! Provides:
//! - SSRF validation reusing `crate::executor::validate_target_url`.
//! - Safe HTTP client with strict timeouts, redirect prevention, and size bounds.
//! - Path traversal prevention for local file inspection.
//! - Root containment for caller-supplied file paths (`resolve_path_within_root`).
//! - Secret sanitization to prevent leaking tokens, passwords, or keys.

use crate::agent_tools::ToolError;
use reqwest::Url;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Validates a URL against AIQA's real SSRF guard (`crate::executor::validate_target_url`).
///
/// Rejects:
/// - non-http/https schemes
/// - loopback / 127.0.0.0/8 / ::1 (unless explicit orchestrator port 8080)
/// - private RFC1918 ranges (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
/// - link-local / cloud metadata (169.254.169.254, fe80::/10)
/// - unspecified / broadcast / multicast addresses
pub async fn validate_safe_url(tool_name: &str, raw_url: &str) -> Result<Url, ToolError> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err(ToolError::InvalidInput {
            tool: tool_name.to_string(),
            details: "URL must not be empty".to_string(),
        });
    }

    let parsed = Url::parse(trimmed).map_err(|e| ToolError::InvalidInput {
        tool: tool_name.to_string(),
        details: format!("invalid URL '{trimmed}': {e}"),
    })?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(ToolError::SafetyViolation {
            tool: tool_name.to_string(),
            reason: format!(
                "disallowed scheme '{}': only http and https are permitted",
                parsed.scheme()
            ),
        });
    }

    if let Err(reason) = crate::executor::validate_target_url(trimmed).await {
        return Err(ToolError::SafetyViolation {
            tool: tool_name.to_string(),
            reason: format!("SSRF protection blocked target: {reason}"),
        });
    }

    Ok(parsed)
}

/// Performs a bounded, safe HTTP GET request with SSRF guard and redirect blocking.
///
/// Ensures:
/// - Redirects are blocked (preventing SSRF redirect pivot).
/// - Request is timeout-bounded.
/// - Response size is bounded by `max_bytes`.
/// - Errors never expose secrets or credentials.
pub async fn safe_http_get(
    tool_name: &str,
    raw_url: &str,
    timeout: Duration,
    max_bytes: usize,
) -> Result<String, ToolError> {
    let url = validate_safe_url(tool_name, raw_url).await?;

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ToolError::ExecutionFailed {
            tool: tool_name.to_string(),
            details: format!("failed to initialize HTTP client: {e}"),
        })?;

    let response = client.get(url.clone()).send().await.map_err(|e| {
        if e.is_timeout() {
            ToolError::Timeout {
                tool: tool_name.to_string(),
                timeout_ms: timeout.as_millis() as u64,
            }
        } else if e.is_redirect() {
            ToolError::SafetyViolation {
                tool: tool_name.to_string(),
                reason: "HTTP redirect rejected: redirects are disabled for security".to_string(),
            }
        } else {
            ToolError::ExecutionFailed {
                tool: tool_name.to_string(),
                details: format!("HTTP request failed: {e}"),
            }
        }
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(ToolError::ExecutionFailed {
            tool: tool_name.to_string(),
            details: format!("HTTP server returned error status: {status}"),
        });
    }

    if let Some(len) = response.content_length() {
        if len as usize > max_bytes {
            return Err(ToolError::ExecutionFailed {
                tool: tool_name.to_string(),
                details: format!(
                    "response size ({len} bytes) exceeds maximum limit ({max_bytes} bytes)"
                ),
            });
        }
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            tool: tool_name.to_string(),
            details: format!("failed to read response stream: {e}"),
        })?;

    if bytes.len() > max_bytes {
        return Err(ToolError::ExecutionFailed {
            tool: tool_name.to_string(),
            details: format!(
                "received bytes ({}) exceeded limit ({max_bytes})",
                bytes.len()
            ),
        });
    }

    String::from_utf8(bytes.to_vec()).map_err(|e| ToolError::InvalidInput {
        tool: tool_name.to_string(),
        details: format!("response is not valid UTF-8 text: {e}"),
    })
}

/// Validates a local file path to prevent path traversal and unauthorized directory access.
pub fn validate_safe_local_path(tool_name: &str, raw_path: &str) -> Result<PathBuf, ToolError> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(ToolError::InvalidInput {
            tool: tool_name.to_string(),
            details: "file path must not be empty".to_string(),
        });
    }

    if trimmed.contains('\0') {
        return Err(ToolError::SafetyViolation {
            tool: tool_name.to_string(),
            reason: "file path contains null byte".to_string(),
        });
    }

    let path = Path::new(trimmed);

    // Prevent path traversal
    for comp in path.components() {
        if let std::path::Component::ParentDir = comp {
            return Err(ToolError::SafetyViolation {
                tool: tool_name.to_string(),
                reason: "directory traversal (..) is strictly forbidden".to_string(),
            });
        }
    }

    let normalized = trimmed.to_lowercase().replace('\\', "/");

    // Reject sensitive system locations
    let blocked_patterns = [
        "/etc/",
        "/proc/",
        "/sys/",
        "c:/windows/",
        "c:/winnt/",
        "c:/program files",
        ".env",
        ".ssh/",
        "id_rsa",
        "id_ed25519",
        "passwd",
        "shadow",
    ];

    for pattern in &blocked_patterns {
        if normalized.contains(pattern) {
            return Err(ToolError::SafetyViolation {
                tool: tool_name.to_string(),
                reason: format!("access to sensitive path pattern '{pattern}' is forbidden"),
            });
        }
    }

    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        // Resolve relative to repo root (manifest dir / ../..)
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest_dir
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(manifest_dir);
        repo_root.join(path)
    };

    Ok(resolved)
}

/// Why `resolve_path_within_root` refused a path. The distinctions are for
/// server logs and tests only: relaying them to an HTTP caller would turn
/// the endpoint into an oracle for which files exist outside the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathContainmentError {
    /// The root itself doesn't resolve to an existing directory.
    RootUnavailable,
    /// A network/device namespace path, or one that resolves to nothing.
    Unresolvable,
    /// Resolves (through symlinks, junctions and `..`) to outside the root.
    OutsideRoot,
    /// Resolves inside the root, but not to a regular file.
    NotARegularFile,
}

/// Positive counterpart to `validate_safe_local_path`: returns the canonical
/// path of a regular file that really lives inside `root`, or refuses.
///
/// The denylist above is lexical, so it never asks where a path actually
/// leads: absolute paths, junctions, UNC shares and anything it doesn't
/// list all pass. Here the OS resolves the path first and only the
/// *result* is compared with the root. `raw_path` is absolute, or relative
/// to `root`.
pub fn resolve_path_within_root(
    raw_path: &str,
    root: &Path,
) -> Result<PathBuf, PathContainmentError> {
    // `\\host\share`, `\\?\…` and `\\.\…` (either slash style) are Windows
    // network/device namespaces: canonicalizing one makes the server open an
    // SMB connection to a caller-chosen host, so refuse before any
    // filesystem call.
    if cfg!(windows) && matches!(raw_path.as_bytes(), [b'\\' | b'/', b'\\' | b'/', ..]) {
        return Err(PathContainmentError::Unresolvable);
    }

    // Both sides are canonicalized: on Windows that yields `\\?\` verbatim
    // paths with on-disk casing, which a root left as configured would
    // never compare equal to.
    let root = root
        .canonicalize()
        .map_err(|_| PathContainmentError::RootUnavailable)?;
    if !root.is_dir() {
        return Err(PathContainmentError::RootUnavailable);
    }

    // `join` keeps a relative path under the root and lets an absolute one
    // replace it; either way the resolved result is what gets judged. A
    // path that doesn't exist can't be resolved, so it is refused, never
    // used as given.
    let resolved = root
        .join(raw_path)
        .canonicalize()
        .map_err(|_| PathContainmentError::Unresolvable)?;

    // Component-wise: `<root>-evil` shares a string prefix with `<root>`
    // but is not inside it.
    if !resolved.starts_with(&root) {
        return Err(PathContainmentError::OutsideRoot);
    }

    // Excludes directories, and on Unix the FIFOs/devices a read would block on.
    if !resolved.is_file() {
        return Err(PathContainmentError::NotARegularFile);
    }
    Ok(resolved)
}

/// Recursively sanitizes JSON value to redact sensitive keys and credentials.
pub fn sanitize_secrets(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                let lower = k.to_lowercase();
                if is_sensitive_key(&lower) {
                    *v = Value::String("[REDACTED]".to_string());
                } else {
                    sanitize_secrets(v);
                }
            }
        }
        Value::Array(list) => {
            for item in list {
                sanitize_secrets(item);
            }
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let sensitive_terms = [
        "authorization",
        "token",
        "secret",
        "password",
        "api_key",
        "apikey",
        "access_token",
        "private_key",
        "bearer",
        "cookie",
        "client_secret",
    ];

    sensitive_terms.iter().any(|term| key.contains(term))
}

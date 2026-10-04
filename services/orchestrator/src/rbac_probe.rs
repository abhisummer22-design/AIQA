//! RBAC testing by measurement, not by declaration.
//!
//! ## Why this exists and not a matrix
//!
//! The obvious thing to build here is a Role-Permission Matrix: a table saying
//! "admin may do X, operator may do Y". That is what a QA person asks for, and it
//! is what this used to be -- a hardcoded table of invented roles and an
//! invented "Evaluate Authorization Matrix" button, removed in FE-11 because it
//! was fabricated.
//!
//! Rebuilding it by reading the handlers does not fix the problem, it moves it.
//! `require_role` lives *inside* handler bodies, so extracting it means parsing
//! Rust, and hand-writing it means going stale. Both were tried here and both
//! produced wrong answers: a hand-written matrix had 18 rows whose methods did
//! not match the router (`/executions` registers POST, the table claimed GET),
//! and two independent source parsers disagreed with each other about how many
//! routes had no role gate at all -- 66 versus 8. One of those parsers also
//! reported `ai-usage` as admin-only, which is false: it answers 200 to a viewer.
//!
//! So this module claims nothing about permissions. It *sends the request*.
//! `run_probe` makes a real HTTP call to this same server with the key it was
//! given and returns what the server actually answered. The answer cannot drift,
//! because it is produced by the code under test at the moment it is asked.
//!
//! ## What a result means, precisely
//!
//! `allowed` is not "the request succeeded". A handler may refuse for its own
//! reasons -- a malformed body gives 400, a missing resource gives 404 -- and
//! none of those are authorization failures. The authorization question is
//! exactly one thing: did the gate refuse?
//!
//! | answer | meaning                                                       |
//! |--------|---------------------------------------------------------------|
//! | 403    | refused at the gate -- `allowed: false`                        |
//! | 401    | the key was rejected; that is a probe error, not a finding     |
//! | 405    | the router does not serve this verb; nothing to test            |
//! | other  | the gate let it through, whatever happened next                |
//!
//! That last row is why `gate` is reported as a name rather than a boolean: "let
//! through and then failed validation" and "let through and succeeded" are very
//! different news, and collapsing them would be the same overclaiming in a
//! softer voice.
//!
//! ## Why a request body is part of the target
//!
//! Authorization runs after body extraction, because `require_role` is called in
//! the handler while `Json<T>` is an argument extractor. So a request whose body
//! the extractor rejects never reaches the gate and comes back 422 instead of
//! 403 -- for *any* role. Probing with `{}` therefore measures the schema, not
//! the permissions. Each target that needs a body carries a minimal valid one so
//! the gate is actually reached.

use crate::auth::Role;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A route worth probing, and why a QA person would probe it.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeTarget {
    /// Stable id, so a saved test report refers to a row that cannot be renamed
    /// out from under it.
    pub id: &'static str,
    pub method: &'static str,
    /// The registered path template, e.g. `/api/v1/projects/:project_id/crawl`.
    pub path: &'static str,
    /// What this route does, in the words a QA report would use.
    pub capability: &'static str,
    /// Which part of the RBAC surface it belongs to.
    pub category: &'static str,
    /// True when reaching the handler could change something. The page warns, and
    /// every id in a mutating target is a non-existent uuid so that if the gate
    /// does its job nothing real is touched.
    pub mutating: bool,
    /// A minimal body that gets past extraction so the gate is reached. `None`
    /// for methods that take no body.
    pub body: Option<&'static str>,
}

/// The minimum body a POST needs to be refused by the gate rather than by the
/// extractor, discovered by probing with a viewer key until the answer turned from
/// 422 (the extractor answered) into 403 (the gate answered).
///
/// Discovering them this way matters: without a body these targets report the
/// request schema instead of the permissions, which is a false reading in the
/// permissive direction -- a viewer looks as though it got past the gate. The
/// field names and types below are what the server asked for, so they are known
/// to be accepted rather than guessed from the struct.
///
/// Values are deliberately obvious (`rbac-probe`, `[]`) so that anything a probe
/// does create is recognisable and disposable.
const CASE_BODY: &str = r#"{"title":"rbac-probe","description":"rbac-probe","category":"rbac-probe","priority":"rbac-probe","severity":"rbac-probe","steps":[]}"#;
const GENERATE_BODY: &str = r#"{"requirement_text":"rbac-probe"}"#;
const RUN_BODY: &str = r#"{"name":"rbac-probe","tasks":[]}"#;
const CRAWL_BODY: &str = r#"{"base_url":"rbac-probe","roles":[]}"#;

pub const TARGETS: &[ProbeTarget] = &[
    // ---- who you are ------------------------------------------------------
    ProbeTarget {
        id: "whoami",
        method: "GET",
        path: "/api/v1/auth/whoami",
        capability: "Report your own key's identity, role and project scope",
        category: "Identity",
        mutating: false,
        body: None,
    },
    // ---- keys: the highest blast radius in the app ------------------------
    ProbeTarget {
        id: "keys-list",
        method: "GET",
        path: "/admin/api-keys",
        capability: "List every API key in the deployment",
        category: "Keys",
        mutating: false,
        body: None,
    },
    ProbeTarget {
        id: "keys-create",
        method: "POST",
        path: "/admin/api-keys",
        capability: "Mint a new API key",
        category: "Keys",
        mutating: true,
        body: Some(r#"{"name":"rbac-probe","role":"viewer"}"#),
    },
    ProbeTarget {
        id: "keys-revoke",
        method: "DELETE",
        path: "/admin/api-keys/:id",
        capability: "Revoke an API key",
        category: "Keys",
        mutating: true,
        body: None,
    },
    ProbeTarget {
        id: "audit-deployment",
        method: "GET",
        path: "/admin/audit-log",
        capability: "Read the deployment-wide audit trail",
        category: "Keys",
        mutating: false,
        body: None,
    },
    // ---- projects ---------------------------------------------------------
    ProbeTarget {
        id: "projects-list",
        method: "GET",
        path: "/api/v1/projects",
        capability: "List every project in the deployment",
        category: "Projects",
        mutating: false,
        body: None,
    },
    ProbeTarget {
        id: "projects-create",
        method: "POST",
        path: "/api/v1/projects",
        capability: "Create a project",
        category: "Projects",
        mutating: true,
        body: Some(r#"{"name":"rbac-probe"}"#),
    },
    // ---- test design ------------------------------------------------------
    ProbeTarget {
        id: "cases-list",
        method: "GET",
        path: "/api/v1/projects/:project_id/test-cases",
        capability: "List a project's test cases",
        category: "Test design",
        mutating: false,
        body: None,
    },
    ProbeTarget {
        id: "cases-create",
        method: "POST",
        path: "/api/v1/projects/:project_id/test-cases",
        capability: "Create a test case by hand",
        category: "Test design",
        mutating: true,
        body: Some(CASE_BODY),
    },
    ProbeTarget {
        id: "cases-generate",
        method: "POST",
        path: "/api/v1/projects/:project_id/test-cases/generate-ai",
        capability: "Generate test cases with AI (spends tokens)",
        category: "Test design",
        mutating: true,
        body: Some(GENERATE_BODY),
    },
    ProbeTarget {
        id: "cases-compile",
        method: "POST",
        path: "/api/v1/projects/:project_id/test-cases/:id/compile",
        capability: "Compile a test case into an executable plan",
        category: "Test design",
        mutating: true,
        body: None,
    },
    // ---- execution --------------------------------------------------------
    ProbeTarget {
        id: "runs-list",
        method: "GET",
        path: "/api/v1/projects/:project_id/executions",
        capability: "List execution runs",
        category: "Execution",
        mutating: false,
        body: None,
    },
    ProbeTarget {
        id: "runs-create",
        method: "POST",
        path: "/api/v1/projects/:project_id/executions",
        capability: "Start an execution run",
        category: "Execution",
        mutating: true,
        body: Some(RUN_BODY),
    },
    ProbeTarget {
        id: "runs-approve",
        method: "POST",
        path: "/api/v1/projects/:project_id/executions/:run_id/approve",
        capability: "Approve a run for dispatch",
        category: "Execution",
        mutating: true,
        body: None,
    },
    // ---- the live application --------------------------------------------
    ProbeTarget {
        id: "crawl-start",
        method: "POST",
        path: "/api/v1/projects/:project_id/crawl",
        capability: "Crawl a running application (drives a real browser)",
        category: "Live application",
        mutating: true,
        body: Some(CRAWL_BODY),
    },
    // ---- records that write to the repository -----------------------------
    ProbeTarget {
        id: "healing-approve",
        method: "POST",
        path: "/api/v1/projects/:project_id/healing/proposals/:proposal_id/approve",
        capability: "Approve a code change (writes to the repository)",
        category: "Repository writes",
        mutating: true,
        body: None,
    },
    ProbeTarget {
        id: "healing-execute",
        method: "POST",
        path: "/api/v1/projects/:project_id/healing/proposals/:proposal_id/execute",
        capability: "Execute an approved healing change (writes to the repository)",
        category: "Repository writes",
        mutating: true,
        body: None,
    },
    ProbeTarget {
        id: "retention-prune",
        method: "POST",
        path: "/api/v1/projects/:project_id/intelligence/retention/prune",
        capability: "Run an audit retention prune (deletes rows)",
        category: "Repository writes",
        mutating: true,
        body: None,
    },
    // ---- analysis ---------------------------------------------------------
    ProbeTarget {
        id: "audit-project",
        method: "GET",
        path: "/api/v1/projects/:project_id/audit",
        capability: "Read a project's audit trail (holds who-did-what)",
        category: "Analysis",
        mutating: false,
        body: None,
    },
    ProbeTarget {
        id: "ai-usage-project",
        method: "GET",
        path: "/api/v1/projects/:project_id/ai-usage",
        capability: "Read one project's AI token usage and cost",
        category: "Analysis",
        mutating: false,
        body: None,
    },
    ProbeTarget {
        id: "dashboard",
        method: "GET",
        path: "/api/v1/projects/:project_id/dashboard",
        capability: "Read the project dashboard",
        category: "Analysis",
        mutating: false,
        body: None,
    },
    // ---- deployment-wide --------------------------------------------------
    ProbeTarget {
        id: "metrics",
        method: "GET",
        path: "/metrics",
        capability: "Prometheus metrics (deployment-wide counters)",
        category: "Deployment",
        mutating: false,
        body: None,
    },
    ProbeTarget {
        id: "ai-usage-deployment",
        method: "GET",
        path: "/api/v1/ai/usage",
        capability: "Read deployment-wide AI usage and cost",
        category: "Deployment",
        mutating: false,
        body: None,
    },
    // ---- what a viewer is meant to reach ----------------------------------
    ProbeTarget {
        id: "logs",
        method: "GET",
        path: "/api/v1/logs",
        capability: "Read orchestrator logs",
        category: "Read-only",
        mutating: false,
        body: None,
    },
    ProbeTarget {
        id: "project-logs",
        method: "GET",
        path: "/api/v1/projects/:project_id/logs",
        capability: "Read a project's run logs",
        category: "Read-only",
        mutating: false,
        body: None,
    },
];

/// An id that cannot exist, so a mutating probe that reaches its handler cannot
/// touch anything real.
pub const GHOST_ID: &str = "00000000-0000-0000-0000-000000000000";

/// Fills a registered template in so it can actually be requested.
pub fn concrete_path(template: &str, project_id: Uuid) -> String {
    template
        .replace(":project_id", &project_id.to_string())
        .replace(":run_id", GHOST_ID)
        .replace(":task_id", GHOST_ID)
        .replace(":proposal_id", GHOST_ID)
        .replace(":evidence_id", GHOST_ID)
        .replace(":finding_id", GHOST_ID)
        .replace(":cluster_id", GHOST_ID)
        .replace(":id", GHOST_ID)
}

/// Why the request was refused, or not. Named rather than boolean because
/// "the gate said no" and "the gate said yes and then something else did" are
/// different results and must not be reported as the same one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    /// `require_role` refused: the role is too weak for this route.
    RoleRefused,
    /// The multi-tenant gate refused: the key is scoped to another project.
    ScopeRefused,
    /// Refused with a 403 whose reason was neither gate.
    RefusedUnknown,
    /// The gate allowed it and the handler then succeeded.
    AllowedSucceeded,
    /// The gate allowed it and the handler then refused for its own reasons.
    AllowedThenRefused,
    /// The router does not serve this verb, so there is nothing to test.
    VerbNotServed,
    /// The key was rejected. A probe error, never a finding.
    KeyRejected,
    /// The probe could not be sent at all.
    ProbeFailed,
}

impl Gate {
    /// The one question a QA reader has: did the gate block this?
    pub fn blocked(self) -> bool {
        matches!(
            self,
            Gate::RoleRefused | Gate::ScopeRefused | Gate::RefusedUnknown
        )
    }

    /// The measurement itself was unsound, so the row must not be read as a
    /// result. A bad key or an unroutable probe proves nothing about permissions.
    pub fn inconclusive(self) -> bool {
        matches!(
            self,
            Gate::VerbNotServed | Gate::KeyRejected | Gate::ProbeFailed
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Gate::RoleRefused => "blocked by the role gate",
            Gate::ScopeRefused => "blocked by the project-scope gate",
            Gate::RefusedUnknown => "refused 403, reason not a known gate",
            Gate::AllowedSucceeded => "allowed, and it worked",
            Gate::AllowedThenRefused => "allowed by the gate, refused afterwards",
            Gate::VerbNotServed => "verb not served for this path",
            Gate::KeyRejected => "the key was rejected (probe error)",
            Gate::ProbeFailed => "the probe could not be sent",
        }
    }
}

/// Classifies a real answer. The message is the server's own, not a guess: the
/// guards word their refusals distinctly, and matching on that wording is what
/// lets this say *which* gate fired instead of only that something did.
pub fn classify(status: u16, body: &str) -> Gate {
    match status {
        401 => return Gate::KeyRejected,
        405 => return Gate::VerbNotServed,
        _ => {}
    }
    if status == 403 {
        if body.contains("cannot perform this action") {
            return Gate::RoleRefused;
        }
        if body.contains("scoped to project") || body.contains("project scope") {
            return Gate::ScopeRefused;
        }
        return Gate::RefusedUnknown;
    }
    if (200..300).contains(&status) {
        Gate::AllowedSucceeded
    } else {
        // 400, 404, 422: the gate did not object, so the role got past it.
        Gate::AllowedThenRefused
    }
}

/// What the probe endpoint accepts.
#[derive(Debug, Deserialize)]
pub struct ProbeRequest {
    /// A target id from `TARGETS`, or `path` to probe something not listed.
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    /// The key to probe with. Passed in rather than looked up so the page can
    /// test roles this server has never heard of.
    pub api_key: String,
    /// The project to substitute into `:project_id`.
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

/// What the probe returns: the server's own answer, classified, and nothing more.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub target_id: Option<String>,
    pub method: String,
    /// The path actually requested, with ids filled in.
    pub path: String,
    pub status: u16,
    pub gate: Gate,
    pub gate_label: &'static str,
    /// The gate did not refuse. Deliberately not called `success`.
    pub allowed: bool,
    /// True when the measurement itself was unsound.
    pub inconclusive: bool,
    /// The server's own words, trimmed.
    pub message: String,
    /// True when this target could change something.
    pub mutating: bool,
}

/// The host the probe is allowed to talk to.
///
/// The path and the key are the caller's to choose; the host is not. Taking it
/// from the caller would turn this endpoint into a way to make the server issue
/// authenticated requests to anything it can reach, which is a far more
/// interesting capability than the one being offered. So only loopback is
/// accepted, and the port comes from the request that arrived here -- the same
/// server, not a guess at a port.
pub fn loopback_base(host_header: Option<&str>) -> Option<String> {
    let host = host_header?.trim();
    let (name, port) = match host.rsplit_once(':') {
        Some((n, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (n, p),
        _ => (host, "80"),
    };
    let bare = name.trim_matches(|c| c == '[' || c == ']');
    if !matches!(bare, "127.0.0.1" | "localhost" | "::1" | "[::1]") {
        return None;
    }
    Some(format!("http://{bare}:{port}"))
}

/// Sends the probe and reports what came back.
pub async fn run_probe(
    base: &str,
    client: &reqwest::Client,
    req: &ProbeRequest,
) -> Result<ProbeResult, String> {
    let listed = req
        .target_id
        .as_deref()
        .and_then(|id| TARGETS.iter().find(|t| t.id == id));

    let method_str = req
        .method
        .clone()
        .or_else(|| listed.map(|t| t.method.to_string()))
        .ok_or_else(|| "say which method to use, or name a target id".to_string())?;

    let template = req
        .path
        .clone()
        .or_else(|| listed.map(|t| t.path.to_string()))
        .ok_or_else(|| "say which path to request, or name a target id".to_string())?;

    let method = reqwest::Method::from_bytes(method_str.to_uppercase().as_bytes())
        .map_err(|_| format!("{method_str} is not an HTTP method"))?;

    if req.api_key.trim().is_empty() {
        return Err("the key to probe with is empty".to_string());
    }

    let path = match req.project_id {
        Some(p) => concrete_path(&template, p),
        None => template.clone(),
    };

    // An absolute URL in `path` would send the probe somewhere the host check
    // does not cover, so only the path and query part is honoured.
    let relative = path
        .strip_prefix("http://")
        .or_else(|| path.strip_prefix("https://"))
        .map(|rest| rest.find('/').map_or("/", |i| &rest[i..]))
        .unwrap_or(&path);
    if !relative.starts_with('/') {
        return Err("the path must start with '/'".to_string());
    }

    let wants_body = matches!(
        method,
        reqwest::Method::POST | reqwest::Method::PUT | reqwest::Method::PATCH
    );

    let mut builder = client
        .request(method, format!("{base}{relative}"))
        .bearer_auth(req.api_key.trim())
        .timeout(std::time::Duration::from_secs(20));
    if wants_body {
        let body = req
            .body
            .clone()
            .or_else(|| {
                listed
                    .and_then(|t| t.body)
                    .and_then(|b| serde_json::from_str(b).ok())
            })
            .unwrap_or_else(|| serde_json::json!({}));
        builder = builder.json(&body);
    }

    let response = builder.send().await.map_err(|e| format!("probe failed: {e}"))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let trimmed = text.trim();
    let message = if trimmed.len() > 400 {
        format!("{}...", &trimmed[..400])
    } else {
        trimmed.to_string()
    };

    let gate = classify(status, &message);
    Ok(ProbeResult {
        target_id: req.target_id.clone(),
        method: method_str.to_uppercase(),
        path: relative.to_string(),
        status,
        gate,
        gate_label: gate.label(),
        allowed: !gate.blocked(),
        inconclusive: gate.inconclusive(),
        message,
        mutating: listed.map(|t| t.mutating).unwrap_or(req.method.is_some()),
    })
}

/// The roles a page should offer, weakest first, so a report reads in order.
pub fn roles_weakest_first() -> Vec<Role> {
    vec![Role::Viewer, Role::Operator, Role::Admin]
}
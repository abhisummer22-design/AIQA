//! Tool 2: DOM Prober (`dom_prober`).
//!
//! Inspects static crawler / DOM snapshots (`CrawlOutput`) to answer element existence,
//! accessibility role, visibility across authenticated roles, attributes, and context questions
//! without launching browsers or executing scripts.
//!
//! Strict Guarantees:
//! - Diagnostic safety level.
//! - Read-only inspection against static `CrawlOutput`.
//! - No browser launch, no Playwright execution, no JavaScript eval, no DOM mutation.
//! - Strict input validation against script injection and executable payloads.
//! - Explicit reporting of unsupported capabilities rather than faking data.

use crate::agent::AgentContext;
use crate::agent_tools::{
    AgentTool, ToolDefinition, ToolError, ToolInvocation, ToolObservation, ToolSafetyLevel,
};
use crate::crawler::{CrawlOutput, CrawledElement, CrawledPage};
use crate::tools::security::sanitize_secrets;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Instant;

const TOOL_NAME: &str = "dom_prober";
const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const MAX_SELECTOR_LEN: usize = 256;
const DEFAULT_MAX_RESULTS: usize = 20;

#[derive(Debug, Clone, Default)]
pub struct DomProberTool;

impl DomProberTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Deserialize)]
struct DomProberArgs {
    selector: String,
    #[serde(default = "default_query_type")]
    query_type: String,
    #[serde(default)]
    crawl_task_id: Option<String>,
    #[serde(default)]
    crawl_output: Option<Value>,
    #[serde(default)]
    role_filter: Option<String>,
    #[serde(default)]
    url_filter: Option<String>,
    #[serde(default)]
    max_results: Option<usize>,
}

fn default_query_type() -> String {
    "existence".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MatchedElementDiagnostic {
    pub role: String,
    pub page_url: String,
    pub page_title: String,
    pub element_index: usize,
    pub kind: String,
    pub text: String,
    pub selector_hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessible_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nearby_elements: Option<Vec<CrawledElementSummary>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrawledElementSummary {
    pub kind: String,
    pub text: String,
    pub selector_hint: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DomProberResult {
    pub selector: String,
    pub query_type: String,
    pub exists: bool,
    pub match_count: usize,
    pub roles_with_match: Vec<String>,
    pub matching_elements: Vec<MatchedElementDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility_details: Option<VisibilityReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_capabilities: Option<Vec<UnsupportedCapabilityNotice>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VisibilityReport {
    pub visible_in_roles: Vec<String>,
    pub absent_in_roles: Vec<String>,
    pub role_access_differential: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnsupportedCapabilityNotice {
    pub capability: String,
    pub reason: String,
}

#[async_trait]
impl AgentTool for DomProberTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: TOOL_NAME.to_string(),
            description: "Inspects static crawler DOM snapshots to answer questions about element existence, role accessibility, visibility across roles, attributes, and surrounding DOM context without launching a browser or running scripts.".to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector hint, ID, class, or text locator to probe (max 256 characters)."
                    },
                    "query_type": {
                        "type": "string",
                        "enum": ["existence", "visibility", "role", "attributes", "context", "all"],
                        "description": "Aspect to probe: 'existence', 'visibility', 'role', 'attributes', 'context', or 'all'."
                    },
                    "crawl_task_id": {
                        "type": "string",
                        "description": "Task ID of a completed crawler run to read from orchestrator disk."
                    },
                    "crawl_output": {
                        "type": "object",
                        "description": "Direct CrawlOutput JSON payload if available."
                    },
                    "role_filter": {
                        "type": "string",
                        "description": "Optional crawl role name to restrict the search."
                    },
                    "url_filter": {
                        "type": "string",
                        "description": "Optional URL substring to restrict search to a specific page."
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of matching elements to return (default 20, max 100)."
                    }
                },
                "required": ["selector"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string" },
                    "query_type": { "type": "string" },
                    "exists": { "type": "boolean" },
                    "match_count": { "type": "integer" },
                    "roles_with_match": { "type": "array", "items": { "type": "string" } },
                    "matching_elements": { "type": "array" }
                },
                "required": ["selector", "query_type", "exists", "match_count"]
            })),
            safety_level: ToolSafetyLevel::Diagnostic,
            default_timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }

    async fn execute(
        &self,
        invocation: &ToolInvocation,
        context: &AgentContext,
    ) -> Result<ToolObservation, ToolError> {
        let start = Instant::now();

        // 1. Cooperative cancellation check
        if context.is_cancelled() {
            return Err(ToolError::Cancelled);
        }

        // 2. Timeout check
        if context.is_timed_out() {
            return Err(ToolError::Timeout {
                tool: TOOL_NAME.to_string(),
                timeout_ms: context.elapsed().as_millis() as u64,
            });
        }

        // 3. Deserialize and validate arguments
        let args: DomProberArgs =
            serde_json::from_value(invocation.arguments.clone()).map_err(|e| {
                ToolError::InvalidInput {
                    tool: TOOL_NAME.to_string(),
                    details: format!("failed to deserialize arguments: {e}"),
                }
            })?;

        let selector = args.selector.trim();
        validate_selector(selector)?;

        // 4. Resolve CrawlOutput
        let crawl: CrawlOutput = if let Some(ref crawl_val) = args.crawl_output {
            serde_json::from_value(crawl_val.clone()).map_err(|e| ToolError::InvalidInput {
                tool: TOOL_NAME.to_string(),
                details: format!("invalid crawl_output schema: {e}"),
            })?
        } else if let Some(ref task_id) = args.crawl_task_id {
            crate::crawler::read_crawl_result(task_id).map_err(|e| {
                ToolError::NotFound(format!(
                    "crawl result for task '{task_id}' could not be read: {e}"
                ))
            })?
        } else {
            return Err(ToolError::InvalidInput {
                tool: TOOL_NAME.to_string(),
                details: "must provide either crawl_output or crawl_task_id".to_string(),
            });
        };

        let max_results = args
            .max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .clamp(1, 100);
        let query_type = args.query_type.to_lowercase();

        // 5. Execute inspection against static crawl pages
        let result = probe_dom(&crawl, selector, &query_type, &args, max_results)?;

        let mut output_json =
            serde_json::to_value(&result).map_err(|e| ToolError::ExecutionFailed {
                tool: TOOL_NAME.to_string(),
                details: format!("failed to serialize DOM probe result: {e}"),
            })?;

        sanitize_secrets(&mut output_json);
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ToolObservation::success(
            &invocation.call_id,
            TOOL_NAME,
            ToolSafetyLevel::Diagnostic,
            output_json,
            duration_ms,
        ))
    }
}

/// Validates selector to prevent injection of executable JavaScript, script tags, or shell code.
fn validate_selector(selector: &str) -> Result<(), ToolError> {
    if selector.is_empty() {
        return Err(ToolError::InvalidInput {
            tool: TOOL_NAME.to_string(),
            details: "selector must not be empty".to_string(),
        });
    }

    if selector.len() > MAX_SELECTOR_LEN {
        return Err(ToolError::InvalidInput {
            tool: TOOL_NAME.to_string(),
            details: format!(
                "selector length ({}) exceeds maximum allowed length ({MAX_SELECTOR_LEN})",
                selector.len()
            ),
        });
    }

    let lower = selector.to_lowercase();
    let dangerous_patterns = [
        "<script",
        "</script",
        "javascript:",
        "eval(",
        "() =>",
        "function(",
        "exec(",
        "__proto__",
        "window.",
        "document.",
        "fetch(",
        "alert(",
        ";",
    ];

    for pattern in &dangerous_patterns {
        if lower.contains(pattern) {
            return Err(ToolError::SafetyViolation {
                tool: TOOL_NAME.to_string(),
                reason: format!(
                    "selector contains forbidden pattern '{pattern}' (executable script injection blocked)"
                ),
            });
        }
    }

    Ok(())
}

fn probe_dom(
    crawl: &CrawlOutput,
    selector: &str,
    query_type: &str,
    args: &DomProberArgs,
    max_results: usize,
) -> Result<DomProberResult, ToolError> {
    let mut matching_elements = Vec::new();
    let mut roles_with_match = HashSet::new();
    let mut all_roles = HashSet::new();

    let sel_norm = selector.to_lowercase();

    for role_res in &crawl.roles {
        all_roles.insert(role_res.role.clone());

        if let Some(ref role_filt) = args.role_filter {
            if !role_res.role.eq_ignore_ascii_case(role_filt) {
                continue;
            }
        }

        for page in &role_res.pages {
            if let Some(ref url_filt) = args.url_filter {
                if !page.url.contains(url_filt) {
                    continue;
                }
            }

            for (elem_idx, elem) in page.elements.iter().enumerate() {
                if is_element_match(elem, &sel_norm) {
                    roles_with_match.insert(role_res.role.clone());

                    if matching_elements.len() < max_results {
                        let accessible_role = map_kind_to_role(&elem.kind);
                        let nearby = if query_type == "context" || query_type == "all" {
                            Some(extract_nearby_elements(page, elem_idx))
                        } else {
                            None
                        };

                        matching_elements.push(MatchedElementDiagnostic {
                            role: role_res.role.clone(),
                            page_url: page.url.clone(),
                            page_title: page.title.clone(),
                            element_index: elem_idx,
                            kind: elem.kind.clone(),
                            text: elem.text.clone(),
                            selector_hint: elem.selector_hint.clone(),
                            href: elem.href.clone(),
                            accessible_role: Some(accessible_role),
                            nearby_elements: nearby,
                        });
                    }
                }
            }
        }
    }

    let match_count = matching_elements.len();
    let exists = match_count > 0;

    // Visibility analysis
    let visibility_details = if query_type == "visibility" || query_type == "all" {
        let visible_in_roles: Vec<String> = roles_with_match.iter().cloned().collect();
        let absent_in_roles: Vec<String> =
            all_roles.difference(&roles_with_match).cloned().collect();
        let role_access_differential = !visible_in_roles.is_empty() && !absent_in_roles.is_empty();
        Some(VisibilityReport {
            visible_in_roles,
            absent_in_roles,
            role_access_differential,
        })
    } else {
        None
    };

    // Explicit unsupported capability reporting
    let mut unsupported = Vec::new();
    if query_type == "attributes" || query_type == "all" {
        unsupported.push(UnsupportedCapabilityNotice {
            capability: "computed_css_styles".to_string(),
            reason: "computed styles require active browser layout engine; not captured in static crawler snapshots".to_string(),
        });
    }

    let unsupported_capabilities = if unsupported.is_empty() {
        None
    } else {
        Some(unsupported)
    };

    let mut roles_list: Vec<String> = roles_with_match.into_iter().collect();
    roles_list.sort();

    Ok(DomProberResult {
        selector: selector.to_string(),
        query_type: query_type.to_string(),
        exists,
        match_count,
        roles_with_match: roles_list,
        matching_elements,
        visibility_details,
        unsupported_capabilities,
    })
}

fn is_element_match(elem: &CrawledElement, sel_lower: &str) -> bool {
    let hint_lower = elem.selector_hint.to_lowercase();
    let text_lower = elem.text.to_lowercase();
    let kind_lower = elem.kind.to_lowercase();

    // Direct match on selector hint
    if hint_lower == sel_lower || hint_lower.contains(sel_lower) {
        return true;
    }

    // Direct match on tag/kind (e.g. "button", "input", "a")
    if kind_lower == sel_lower {
        return true;
    }

    // Match on text locator (e.g. text=Login or plain string match)
    if let Some(stripped) = sel_lower.strip_prefix("text=") {
        if text_lower == stripped || text_lower.contains(stripped) {
            return true;
        }
    }

    // If selector is an ID (e.g. "#login-btn")
    if let Some(id_part) = sel_lower.strip_prefix('#') {
        if hint_lower.contains(&format!("#{id_part}"))
            || hint_lower.contains(&format!("id=\"{id_part}\""))
        {
            return true;
        }
    }

    // If selector is a class (e.g. ".btn-primary")
    if let Some(class_part) = sel_lower.strip_prefix('.') {
        if hint_lower.contains(&format!(".{class_part}"))
            || hint_lower.contains(&format!("class=\"{class_part}\""))
        {
            return true;
        }
    }

    false
}

fn map_kind_to_role(kind: &str) -> String {
    match kind.to_lowercase().as_str() {
        "button" => "button".to_string(),
        "input" => "textbox".to_string(),
        "a" | "link" => "link".to_string(),
        "heading" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading".to_string(),
        "checkbox" => "checkbox".to_string(),
        "radio" => "radio".to_string(),
        "select" => "combobox".to_string(),
        "table" => "table".to_string(),
        "form" => "form".to_string(),
        other => format!("role({other})"),
    }
}

fn extract_nearby_elements(page: &CrawledPage, current_idx: usize) -> Vec<CrawledElementSummary> {
    let start = current_idx.saturating_sub(2);
    let end = (current_idx + 3).min(page.elements.len());

    page.elements[start..end]
        .iter()
        .map(|e| CrawledElementSummary {
            kind: e.kind.clone(),
            text: e.text.clone(),
            selector_hint: e.selector_hint.clone(),
        })
        .collect()
}

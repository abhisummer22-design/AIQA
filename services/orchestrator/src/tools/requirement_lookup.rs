//! Tool 3: Requirement Lookup (`requirement_lookup`).
//!
//! Retrieves project requirements, traceability associations, quality-graph edges,
//! and related test cases within authorized multi-tenant project boundaries.
//!
//! Strict Guarantees:
//! - ReadOnly safety level.
//! - Multi-tenant project boundary enforcement (rejects cross-project access).
//! - Role-based authorization verification against `AgentContext.auth`.
//! - Bounded database queries and observation truncation.
//! - Secret sanitization.

use crate::agent::AgentContext;
use crate::agent_tools::{
    AgentTool, ToolDefinition, ToolError, ToolInvocation, ToolObservation, ToolSafetyLevel,
};
use crate::store::{Store, TestCaseRow};
use crate::tools::security::sanitize_secrets;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::collections::BTreeMap;
use std::fmt;
use std::time::Instant;

const TOOL_NAME: &str = "requirement_lookup";
const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 50;

#[derive(Clone, Default)]
pub struct RequirementLookupTool {
    store: Option<Store>,
}

impl fmt::Debug for RequirementLookupTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequirementLookupTool")
            .field("has_store", &self.store.is_some())
            .finish()
    }
}

impl RequirementLookupTool {
    pub fn new(store: Option<Store>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
struct RequirementLookupArgs {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default = "default_query_type")]
    query_type: String,
    #[serde(default)]
    requirement_id: Option<String>,
    #[serde(default)]
    keyword: Option<String>,
    #[serde(default = "default_include_cases")]
    include_test_cases: bool,
    #[serde(default)]
    limit: Option<usize>,
}

fn default_query_type() -> String {
    "keyword".to_string()
}

fn default_include_cases() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RelatedTestCaseSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub category: String,
    pub severity: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequirementRecord {
    pub id: String,
    pub reference: Option<String>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_node_id: Option<String>,
    pub related_test_cases: Vec<RelatedTestCaseSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequirementLookupResult {
    pub project_id: String,
    pub query_type: String,
    pub total_found: usize,
    pub requirements: Vec<RequirementRecord>,
    pub is_truncated: bool,
}

#[async_trait]
impl AgentTool for RequirementLookupTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: TOOL_NAME.to_string(),
            description: "Retrieves project requirements and associated test cases from the Quality Graph and database within authorized project boundaries.".to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "string",
                        "description": "Target project ID to query. If omitted, uses AgentContext.project_id."
                    },
                    "query_type": {
                        "type": "string",
                        "enum": ["id", "keyword", "all"],
                        "description": "Query method: 'id' for exact requirement reference, 'keyword' for search, or 'all'."
                    },
                    "requirement_id": {
                        "type": "string",
                        "description": "Exact requirement key or reference (e.g. 'REQ-042' or 'text:<sha256>')."
                    },
                    "keyword": {
                        "type": "string",
                        "description": "Search term matching requirement text, reference, or title."
                    },
                    "include_test_cases": {
                        "type": "boolean",
                        "description": "Whether to fetch associated test cases for each requirement (default true)."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of requirements to return (default 10, max 50)."
                    }
                }
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "query_type": { "type": "string" },
                    "total_found": { "type": "integer" },
                    "requirements": { "type": "array" }
                },
                "required": ["project_id", "query_type", "total_found", "requirements"]
            })),
            safety_level: ToolSafetyLevel::ReadOnly,
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
        let args: RequirementLookupArgs = serde_json::from_value(invocation.arguments.clone())
            .map_err(|e| ToolError::InvalidInput {
                tool: TOOL_NAME.to_string(),
                details: format!("failed to deserialize arguments: {e}"),
            })?;

        // 4. Resolve and validate target project
        let target_project = match (&args.project_id, &context.project_id) {
            (Some(pid), _) => pid.clone(),
            (None, Some(pid)) => pid.clone(),
            (None, None) => {
                return Err(ToolError::InvalidInput {
                    tool: TOOL_NAME.to_string(),
                    details: "target project_id must be provided in arguments or AgentContext"
                        .to_string(),
                });
            }
        };

        // 5. Multi-tenant Authorization boundary enforcement
        if !context.is_authorized_for_project(&target_project) {
            return Err(ToolError::SafetyViolation {
                tool: TOOL_NAME.to_string(),
                reason: format!(
                    "cross-project authorization violation: agent context cannot access project '{target_project}'"
                ),
            });
        }

        // 6. Resolve Store
        let store = self.store.as_ref().or(context.store.as_ref());

        let limit = args.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let query_type = args.query_type.to_lowercase();

        // 7. Execute lookup
        let result = if let Some(st) = store {
            query_requirements(st, &target_project, &query_type, &args, limit).await?
        } else {
            // Standalone or mock environment without database pool
            RequirementLookupResult {
                project_id: target_project,
                query_type,
                total_found: 0,
                requirements: Vec::new(),
                is_truncated: false,
            }
        };

        let mut output_json =
            serde_json::to_value(&result).map_err(|e| ToolError::ExecutionFailed {
                tool: TOOL_NAME.to_string(),
                details: format!("failed to serialize requirement result: {e}"),
            })?;

        sanitize_secrets(&mut output_json);
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ToolObservation::success(
            &invocation.call_id,
            TOOL_NAME,
            ToolSafetyLevel::ReadOnly,
            output_json,
            duration_ms,
        ))
    }
}

async fn query_requirements(
    store: &Store,
    project_id: &str,
    query_type: &str,
    args: &RequirementLookupArgs,
    limit: usize,
) -> Result<RequirementLookupResult, ToolError> {
    let mut requirements_map: BTreeMap<String, RequirementRecord> = BTreeMap::new();

    // Branch 1: Exact ID Lookup
    if query_type == "id" || args.requirement_id.is_some() {
        let req_id = args
            .requirement_id
            .as_deref()
            .or(args.keyword.as_deref())
            .unwrap_or("");

        if !req_id.is_empty() {
            // 1. Query Quality Graph for requirement node
            let qn_row = sqlx::query(
                "SELECT id, name, external_ref FROM quality_nodes WHERE project_id = ? AND node_type = 'requirement' AND (external_ref = ? OR name = ?)",
            )
            .bind(project_id)
            .bind(req_id)
            .bind(req_id)
            .fetch_optional(&store.pool)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                tool: TOOL_NAME.to_string(),
                details: format!("database error querying quality nodes: {e}"),
            })?;

            let mut linked_cases = Vec::new();
            if args.include_test_cases {
                if let Ok(cases) = store
                    .list_test_cases_for_requirement(project_id, req_id)
                    .await
                {
                    for tc in cases {
                        linked_cases.push(test_case_to_summary(tc));
                    }
                }
            }

            if let Some(row) = qn_row {
                let node_id: String = row.get("id");
                let name: String = row.get("name");
                let ext_ref: Option<String> = row.get("external_ref");

                requirements_map.insert(
                    req_id.to_string(),
                    RequirementRecord {
                        id: req_id.to_string(),
                        reference: ext_ref,
                        title: name,
                        summary: None,
                        quality_node_id: Some(node_id),
                        related_test_cases: linked_cases,
                    },
                );
            } else if !linked_cases.is_empty() {
                // If not in quality_nodes yet but present via test case relations
                requirements_map.insert(
                    req_id.to_string(),
                    RequirementRecord {
                        id: req_id.to_string(),
                        reference: Some(req_id.to_string()),
                        title: req_id.to_string(),
                        summary: None,
                        quality_node_id: None,
                        related_test_cases: linked_cases,
                    },
                );
            }
        }
    }

    // Branch 2: Keyword / Search
    if query_type == "keyword" || query_type == "all" || requirements_map.is_empty() {
        let kw_pattern = if let Some(ref kw) = args.keyword {
            format!("%{}%", kw.trim())
        } else {
            "%".to_string()
        };

        // Query quality nodes matching keyword
        let qn_rows = sqlx::query(
            "SELECT id, name, external_ref FROM quality_nodes WHERE project_id = ? AND node_type = 'requirement' AND (name LIKE ? OR external_ref LIKE ?) LIMIT ?",
        )
        .bind(project_id)
        .bind(&kw_pattern)
        .bind(&kw_pattern)
        .bind(limit as i64)
        .fetch_all(&store.pool)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            tool: TOOL_NAME.to_string(),
            details: format!("database error querying quality nodes by keyword: {e}"),
        })?;

        for row in qn_rows {
            let node_id: String = row.get("id");
            let name: String = row.get("name");
            let ext_ref: Option<String> = row.get("external_ref");
            let key = ext_ref.clone().unwrap_or_else(|| name.clone());

            let mut linked_cases = Vec::new();
            if args.include_test_cases {
                if let Ok(cases) = store
                    .list_test_cases_for_requirement(project_id, &key)
                    .await
                {
                    for tc in cases {
                        linked_cases.push(test_case_to_summary(tc));
                    }
                }
            }

            requirements_map
                .entry(key.clone())
                .or_insert_with(|| RequirementRecord {
                    id: key,
                    reference: ext_ref,
                    title: name,
                    summary: None,
                    quality_node_id: Some(node_id),
                    related_test_cases: linked_cases,
                });
        }

        // Also search test cases table directly for requirement references
        let tc_rows = sqlx::query(
            "SELECT id, project_id, title, description, category, priority, severity, \
             preconditions, steps, expected_result, assertions, test_data, source_document, \
             requirement_reference, source_requirement, traceability, risk_factor, tags, source, status, \
             validation_notes, executed_run_id, executed_task_id, created_by, created_at, updated_at, suite_id \
             FROM test_cases \
             WHERE project_id = ? AND (requirement_reference LIKE ? OR source_requirement LIKE ? OR title LIKE ?) \
             LIMIT ?",
        )
        .bind(project_id)
        .bind(&kw_pattern)
        .bind(&kw_pattern)
        .bind(&kw_pattern)
        .bind((limit * 2) as i64)
        .fetch_all(&store.pool)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            tool: TOOL_NAME.to_string(),
            details: format!("database error querying test cases by requirement: {e}"),
        })?;

        for row in tc_rows {
            let req_ref = row.get::<Option<String>, _>("requirement_reference");
            let src_req = row.get::<Option<String>, _>("source_requirement");

            let ref_key = req_ref
                .clone()
                .or_else(|| src_req.clone())
                .unwrap_or_else(|| "unreferenced".to_string());

            let summary = test_case_row_to_summary(&row);

            let entry =
                requirements_map
                    .entry(ref_key.clone())
                    .or_insert_with(|| RequirementRecord {
                        id: ref_key.clone(),
                        reference: req_ref,
                        title: format!("Requirement {ref_key}"),
                        summary: src_req,
                        quality_node_id: None,
                        related_test_cases: Vec::new(),
                    });

            if !entry.related_test_cases.iter().any(|c| c.id == summary.id) {
                entry.related_test_cases.push(summary);
            }
        }
    }

    let mut requirements: Vec<RequirementRecord> = requirements_map.into_values().collect();
    let total_found = requirements.len();
    let is_truncated = total_found > limit;
    requirements.truncate(limit);

    Ok(RequirementLookupResult {
        project_id: project_id.to_string(),
        query_type: query_type.to_string(),
        total_found,
        requirements,
        is_truncated,
    })
}

fn test_case_to_summary(tc: TestCaseRow) -> RelatedTestCaseSummary {
    RelatedTestCaseSummary {
        id: tc.id.to_string(),
        title: tc.title,
        status: tc.status.as_db_str().to_string(),
        priority: tc.priority,
        category: tc.category,
        severity: tc.severity,
    }
}

fn test_case_row_to_summary(row: &sqlx::sqlite::SqliteRow) -> RelatedTestCaseSummary {
    RelatedTestCaseSummary {
        id: row.get("id"),
        title: row.get("title"),
        status: row.get("status"),
        priority: row.get("priority"),
        category: row.get("category"),
        severity: row.get("severity"),
    }
}

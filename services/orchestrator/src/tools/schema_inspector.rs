//! Tool 1: Schema Inspector (`schema_inspector`).
//!
//! Safely inspects OpenAPI / Swagger definitions (from inline JSON/YAML, local files, or remote URLs)
//! and provides structured path, parameter, request, and response schemas for agent reasoning.
//!
//! Strict Guarantees:
//! - ReadOnly safety level.
//! - SSRF protected via `crate::tools::security::safe_http_get`.
//! - Path traversal protected via `crate::tools::security::validate_safe_local_path`.
//! - Timeout and response size bounded.
//! - Zero external dependencies added.

use crate::agent::AgentContext;
use crate::agent_tools::{
    AgentTool, ToolDefinition, ToolError, ToolInvocation, ToolObservation, ToolSafetyLevel,
};
use crate::tools::security::{safe_http_get, sanitize_secrets, validate_safe_local_path};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::time::{Duration, Instant};

const TOOL_NAME: &str = "schema_inspector";
const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const MAX_SCHEMA_BYTES: usize = 512 * 1024; // 512 KB
const DEFAULT_MAX_PATHS: usize = 50;

#[derive(Debug, Clone, Default)]
pub struct SchemaInspectorTool;

impl SchemaInspectorTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Deserialize)]
struct SchemaInspectorArgs {
    #[serde(default)]
    source_type: Option<String>,
    #[serde(default)]
    schema_content: Option<String>,
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    path_filter: Option<String>,
    #[serde(default)]
    max_paths: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StructuredParameter {
    pub name: String,
    pub r#in: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StructuredRequestBody {
    pub required: bool,
    pub content_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StructuredResponse {
    pub status_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub content_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StructuredOperation {
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Vec<StructuredParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<StructuredRequestBody>,
    pub responses: Vec<StructuredResponse>,
    pub auth_required: bool,
    pub auth_schemes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StructuredPath {
    pub path: String,
    pub methods: Vec<String>,
    pub operations: Vec<StructuredOperation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StructuredSecurityScheme {
    pub name: String,
    pub scheme_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaInspectionResult {
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub servers: Vec<String>,
    pub total_paths_in_spec: usize,
    pub returned_paths_count: usize,
    pub is_truncated: bool,
    pub paths: Vec<StructuredPath>,
    pub security_schemes: Vec<StructuredSecurityScheme>,
}

#[async_trait]
impl AgentTool for SchemaInspectorTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: TOOL_NAME.to_string(),
            description: "Safely inspects an OpenAPI or Swagger API definition (from inline JSON/YAML, local file, or remote URL) and returns structured path, method, parameter, request, and response schemas for QA reasoning.".to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "source_type": {
                        "type": "string",
                        "enum": ["inline", "file", "url"],
                        "description": "Source of the schema ('inline', 'file', or 'url'). Inferred if omitted."
                    },
                    "schema_content": {
                        "type": "string",
                        "description": "Raw OpenAPI JSON or YAML schema text (for source_type 'inline')."
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Path to local schema file (e.g., 'executors/schemathesis/openapi.yaml')."
                    },
                    "url": {
                        "type": "string",
                        "description": "Remote URL serving the OpenAPI JSON/YAML document (SSRF protected)."
                    },
                    "path_filter": {
                        "type": "string",
                        "description": "Optional substring or prefix to filter specific API paths (e.g. '/users')."
                    },
                    "max_paths": {
                        "type": "integer",
                        "description": "Maximum number of paths to return (default 50)."
                    }
                }
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "version": { "type": "string" },
                    "servers": { "type": "array", "items": { "type": "string" } },
                    "total_paths_in_spec": { "type": "integer" },
                    "paths": { "type": "array" },
                    "security_schemes": { "type": "array" }
                },
                "required": ["title", "version", "paths"]
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

        // 2. Timeout boundary check
        if context.is_timed_out() {
            return Err(ToolError::Timeout {
                tool: TOOL_NAME.to_string(),
                timeout_ms: context.elapsed().as_millis() as u64,
            });
        }

        let timeout = context
            .remaining_time()
            .min(Duration::from_millis(DEFAULT_TIMEOUT_MS));

        // 3. Parse input arguments
        let args: SchemaInspectorArgs = serde_json::from_value(invocation.arguments.clone())
            .map_err(|e| ToolError::InvalidInput {
                tool: TOOL_NAME.to_string(),
                details: format!("failed to deserialize arguments: {e}"),
            })?;

        // 4. Resolve source content
        let raw_content = match (
            args.source_type.as_deref(),
            &args.schema_content,
            &args.file_path,
            &args.url,
        ) {
            (_, Some(content), _, _) => {
                if content.len() > MAX_SCHEMA_BYTES {
                    return Err(ToolError::ExecutionFailed {
                        tool: TOOL_NAME.to_string(),
                        details: format!(
                            "schema content size ({} bytes) exceeds maximum limit ({} bytes)",
                            content.len(),
                            MAX_SCHEMA_BYTES
                        ),
                    });
                }
                content.clone()
            }
            (Some("file"), _, Some(path), _) | (_, None, Some(path), None) => {
                let safe_path = validate_safe_local_path(TOOL_NAME, path)?;
                if !safe_path.exists() {
                    return Err(ToolError::NotFound(format!(
                        "local schema file not found: {}",
                        safe_path.display()
                    )));
                }
                let metadata =
                    std::fs::metadata(&safe_path).map_err(|e| ToolError::ExecutionFailed {
                        tool: TOOL_NAME.to_string(),
                        details: format!("failed to read schema file metadata: {e}"),
                    })?;
                if metadata.len() as usize > MAX_SCHEMA_BYTES {
                    return Err(ToolError::ExecutionFailed {
                        tool: TOOL_NAME.to_string(),
                        details: format!(
                            "schema file size ({} bytes) exceeds maximum limit ({} bytes)",
                            metadata.len(),
                            MAX_SCHEMA_BYTES
                        ),
                    });
                }
                std::fs::read_to_string(&safe_path).map_err(|e| ToolError::ExecutionFailed {
                    tool: TOOL_NAME.to_string(),
                    details: format!("failed to read local schema file: {e}"),
                })?
            }
            (Some("url"), _, _, Some(url)) | (_, None, None, Some(url)) => {
                safe_http_get(TOOL_NAME, url, timeout, MAX_SCHEMA_BYTES).await?
            }
            _ => {
                return Err(ToolError::InvalidInput {
                    tool: TOOL_NAME.to_string(),
                    details: "must provide one of: schema_content, file_path, or url".to_string(),
                });
            }
        };

        // 5. Parse content (JSON or YAML)
        let root_val = parse_openapi_document(&raw_content)?;

        // 6. Extract structured representation
        let max_paths = args.max_paths.unwrap_or(DEFAULT_MAX_PATHS).clamp(1, 200);
        let result = extract_structured_schema(&root_val, args.path_filter.as_deref(), max_paths)?;

        let mut output_json =
            serde_json::to_value(&result).map_err(|e| ToolError::ExecutionFailed {
                tool: TOOL_NAME.to_string(),
                details: format!("failed to serialize schema result: {e}"),
            })?;

        // 7. Sanitize secrets
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

/// Parses an OpenAPI document from either JSON or YAML.
pub fn parse_openapi_document(raw: &str) -> Result<Value, ToolError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ToolError::InvalidInput {
            tool: TOOL_NAME.to_string(),
            details: "schema text is empty".to_string(),
        });
    }

    let root = if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        if v.is_object() {
            v
        } else {
            return Err(ToolError::InvalidInput {
                tool: TOOL_NAME.to_string(),
                details: "schema JSON root must be an object".to_string(),
            });
        }
    } else {
        match parse_simple_yaml(trimmed) {
            Ok(v) => {
                if v.is_object() {
                    v
                } else {
                    return Err(ToolError::InvalidInput {
                        tool: TOOL_NAME.to_string(),
                        details: "parsed YAML schema root must be an object/mapping".to_string(),
                    });
                }
            }
            Err(e) => {
                return Err(ToolError::InvalidInput {
                    tool: TOOL_NAME.to_string(),
                    details: format!("malformed schema: failed to parse as JSON or YAML: {e}"),
                });
            }
        }
    };

    // Verify it is an actual OpenAPI / Swagger document
    let is_valid_openapi = root.as_object().map_or(false, |m| {
        m.contains_key("openapi") || m.contains_key("swagger") || m.contains_key("paths")
    });

    if !is_valid_openapi {
        return Err(ToolError::InvalidInput {
            tool: TOOL_NAME.to_string(),
            details: "malformed schema: missing required OpenAPI/Swagger root fields ('openapi', 'swagger', or 'paths')".to_string(),
        });
    }

    Ok(root)
}

/// Extracts a structured schema view from the OpenAPI/Swagger JSON Value.
fn extract_structured_schema(
    root: &Value,
    path_filter: Option<&str>,
    max_paths: usize,
) -> Result<SchemaInspectionResult, ToolError> {
    let info = root.get("info");
    let title = info
        .and_then(|i| i.get("title"))
        .and_then(|t| t.as_str())
        .unwrap_or("Untitled API")
        .to_string();

    let version = info
        .and_then(|i| i.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let description = info
        .and_then(|i| i.get("description"))
        .and_then(|d| d.as_str())
        .map(ToString::to_string);

    let mut servers = Vec::new();
    if let Some(srv_arr) = root.get("servers").and_then(|s| s.as_array()) {
        for srv in srv_arr {
            if let Some(u) = srv.get("url").and_then(|u| u.as_str()) {
                servers.push(u.to_string());
            }
        }
    } else if let Some(base_path) = root.get("basePath").and_then(|b| b.as_str()) {
        // Swagger 2.0
        let host = root.get("host").and_then(|h| h.as_str()).unwrap_or("");
        servers.push(format!("{host}{base_path}"));
    }

    // Paths extraction
    let mut structured_paths = Vec::new();
    let empty_map = Map::new();
    let paths_obj = root
        .get("paths")
        .and_then(|p| p.as_object())
        .unwrap_or(&empty_map);

    let total_paths_in_spec = paths_obj.len();
    let mut returned_count = 0;
    let mut is_truncated = false;

    let http_methods = ["get", "post", "put", "delete", "patch", "options", "head"];

    for (path_str, path_item) in paths_obj {
        if let Some(filter) = path_filter {
            if !path_str.contains(filter) {
                continue;
            }
        }

        if returned_count >= max_paths {
            is_truncated = true;
            break;
        }

        let mut methods = Vec::new();
        let mut operations = Vec::new();

        if let Some(path_map) = path_item.as_object() {
            for method in &http_methods {
                if let Some(op_val) = path_map.get(*method) {
                    methods.push(method.to_uppercase());

                    let operation_id = op_val
                        .get("operationId")
                        .and_then(|o| o.as_str())
                        .map(ToString::to_string);
                    let summary = op_val
                        .get("summary")
                        .and_then(|s| s.as_str())
                        .map(ToString::to_string);
                    let desc = op_val
                        .get("description")
                        .and_then(|d| d.as_str())
                        .map(ToString::to_string);

                    // Parameters
                    let mut parameters = Vec::new();
                    if let Some(param_arr) = op_val.get("parameters").and_then(|p| p.as_array()) {
                        for p in param_arr {
                            if let Some(name) = p.get("name").and_then(|n| n.as_str()) {
                                let r#in = p
                                    .get("in")
                                    .and_then(|i| i.as_str())
                                    .unwrap_or("query")
                                    .to_string();
                                let required =
                                    p.get("required").and_then(|r| r.as_bool()).unwrap_or(false);
                                let p_desc = p
                                    .get("description")
                                    .and_then(|d| d.as_str())
                                    .map(ToString::to_string);
                                let p_type = p
                                    .get("schema")
                                    .and_then(|s| s.get("type"))
                                    .or_else(|| p.get("type"))
                                    .and_then(|t| t.as_str())
                                    .map(ToString::to_string);

                                parameters.push(StructuredParameter {
                                    name: name.to_string(),
                                    r#in,
                                    required,
                                    param_type: p_type,
                                    description: p_desc,
                                });
                            }
                        }
                    }

                    // Request Body
                    let mut request_body = None;
                    if let Some(rb) = op_val.get("requestBody") {
                        let required = rb
                            .get("required")
                            .and_then(|r| r.as_bool())
                            .unwrap_or(false);
                        let mut content_types = Vec::new();
                        let mut body_schema = None;
                        if let Some(content) = rb.get("content").and_then(|c| c.as_object()) {
                            for (ct, ct_obj) in content {
                                content_types.push(ct.clone());
                                if body_schema.is_none() {
                                    body_schema = ct_obj.get("schema").cloned();
                                }
                            }
                        }
                        request_body = Some(StructuredRequestBody {
                            required,
                            content_types,
                            schema: body_schema,
                        });
                    }

                    // Responses
                    let mut responses = Vec::new();
                    if let Some(resp_map) = op_val.get("responses").and_then(|r| r.as_object()) {
                        for (status, resp_val) in resp_map {
                            let resp_desc = resp_val
                                .get("description")
                                .and_then(|d| d.as_str())
                                .map(ToString::to_string);
                            let mut resp_content_types = Vec::new();
                            let mut resp_schema = None;

                            if let Some(content) =
                                resp_val.get("content").and_then(|c| c.as_object())
                            {
                                for (ct, ct_obj) in content {
                                    resp_content_types.push(ct.clone());
                                    if resp_schema.is_none() {
                                        resp_schema = ct_obj.get("schema").cloned();
                                    }
                                }
                            } else if let Some(schema) = resp_val.get("schema") {
                                // Swagger 2.0
                                resp_schema = Some(schema.clone());
                            }

                            responses.push(StructuredResponse {
                                status_code: status.clone(),
                                description: resp_desc,
                                content_types: resp_content_types,
                                schema: resp_schema,
                            });
                        }
                    }

                    // Security requirements
                    let mut auth_required = false;
                    let mut auth_schemes = Vec::new();
                    if let Some(sec_arr) = op_val
                        .get("security")
                        .or_else(|| root.get("security"))
                        .and_then(|s| s.as_array())
                    {
                        if !sec_arr.is_empty() {
                            auth_required = true;
                            for sec in sec_arr {
                                if let Some(sec_obj) = sec.as_object() {
                                    for key in sec_obj.keys() {
                                        if !auth_schemes.contains(key) {
                                            auth_schemes.push(key.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    operations.push(StructuredOperation {
                        method: method.to_uppercase(),
                        operation_id,
                        summary,
                        description: desc,
                        parameters,
                        request_body,
                        responses,
                        auth_required,
                        auth_schemes,
                    });
                }
            }
        }

        structured_paths.push(StructuredPath {
            path: path_str.clone(),
            methods,
            operations,
        });

        returned_count += 1;
    }

    // Security Schemes
    let mut security_schemes = Vec::new();
    let schemes_obj = root
        .get("components")
        .and_then(|c| c.get("securitySchemes"))
        .or_else(|| root.get("securityDefinitions"))
        .and_then(|s| s.as_object());

    if let Some(schemes) = schemes_obj {
        for (name, scheme_val) in schemes {
            let scheme_type = scheme_val
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown")
                .to_string();
            let scheme_desc = scheme_val
                .get("description")
                .and_then(|d| d.as_str())
                .map(ToString::to_string);
            security_schemes.push(StructuredSecurityScheme {
                name: name.clone(),
                scheme_type,
                description: scheme_desc,
            });
        }
    }

    Ok(SchemaInspectionResult {
        title,
        version,
        description,
        servers,
        total_paths_in_spec,
        returned_paths_count: structured_paths.len(),
        is_truncated,
        paths: structured_paths,
        security_schemes,
    })
}

// ---------------------------------------------------------------------------
// Lightweight YAML-to-JSON parser
// ---------------------------------------------------------------------------

/// Parses standard structured YAML (mappings, sequences, scalars) into `serde_json::Value`.
pub fn parse_simple_yaml(input: &str) -> Result<Value, String> {
    let lines: Vec<&str> = input.lines().collect();
    let mut idx = 0;
    parse_yaml_lines(&lines, &mut idx, 0)
}

fn parse_yaml_lines(lines: &[&str], idx: &mut usize, min_indent: usize) -> Result<Value, String> {
    // Skip empty or comment lines
    skip_blank_and_comments(lines, idx);
    if *idx >= lines.len() {
        return Ok(Value::Null);
    }

    let current_line = lines[*idx];
    let indent = line_indent(current_line);
    if indent < min_indent {
        return Ok(Value::Null);
    }

    let trimmed = current_line.trim_start();
    if trimmed.starts_with("- ") || trimmed == "-" {
        parse_yaml_sequence(lines, idx, indent)
    } else if trimmed.contains(':') {
        parse_yaml_mapping(lines, idx, indent)
    } else {
        *idx += 1;
        Ok(parse_yaml_scalar(trimmed))
    }
}

fn parse_yaml_mapping(
    lines: &[&str],
    idx: &mut usize,
    block_indent: usize,
) -> Result<Value, String> {
    let mut map = Map::new();

    while *idx < lines.len() {
        skip_blank_and_comments(lines, idx);
        if *idx >= lines.len() {
            break;
        }

        let line = lines[*idx];
        let indent = line_indent(line);
        if indent < block_indent {
            break;
        }

        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") {
            break;
        }

        if let Some(colon_pos) = find_unquoted_colon(trimmed) {
            let key = trimmed[..colon_pos]
                .trim()
                .trim_matches(|c| c == '\'' || c == '"');
            let rest = trimmed[colon_pos + 1..].trim();
            *idx += 1;

            if rest.is_empty() {
                // Nested structure on subsequent lines
                skip_blank_and_comments(lines, idx);
                if *idx < lines.len() {
                    let next_indent = line_indent(lines[*idx]);
                    if next_indent > indent {
                        let child_val = parse_yaml_lines(lines, idx, next_indent)?;
                        map.insert(key.to_string(), child_val);
                    } else {
                        map.insert(key.to_string(), Value::Null);
                    }
                } else {
                    map.insert(key.to_string(), Value::Null);
                }
            } else {
                let scalar_val = parse_yaml_scalar(rest);
                map.insert(key.to_string(), scalar_val);
            }
        } else {
            *idx += 1;
        }
    }

    Ok(Value::Object(map))
}

fn parse_yaml_sequence(
    lines: &[&str],
    idx: &mut usize,
    block_indent: usize,
) -> Result<Value, String> {
    let mut list = Vec::new();

    while *idx < lines.len() {
        skip_blank_and_comments(lines, idx);
        if *idx >= lines.len() {
            break;
        }

        let line = lines[*idx];
        let indent = line_indent(line);
        if indent < block_indent {
            break;
        }

        let trimmed = line.trim_start();
        if !trimmed.starts_with("- ") && trimmed != "-" {
            break;
        }

        let item_content = if trimmed == "-" { "" } else { &trimmed[2..] }.trim();

        *idx += 1;

        if item_content.is_empty() {
            skip_blank_and_comments(lines, idx);
            if *idx < lines.len() {
                let next_indent = line_indent(lines[*idx]);
                if next_indent > indent {
                    let child = parse_yaml_lines(lines, idx, next_indent)?;
                    list.push(child);
                } else {
                    list.push(Value::Null);
                }
            } else {
                list.push(Value::Null);
            }
        } else if let Some(colon_pos) = find_unquoted_colon(item_content) {
            // Sequence of inline mappings (e.g. "- name: val")
            let key = item_content[..colon_pos]
                .trim()
                .trim_matches(|c| c == '\'' || c == '"');
            let rest = item_content[colon_pos + 1..].trim();
            let mut map = Map::new();
            map.insert(key.to_string(), parse_yaml_scalar(rest));

            // Check if more mapping lines follow at same or deeper indent
            let item_inner_indent = indent + 2;
            while *idx < lines.len() {
                skip_blank_and_comments(lines, idx);
                if *idx >= lines.len() {
                    break;
                }
                let next_line = lines[*idx];
                let next_indent = line_indent(next_line);
                if next_indent < item_inner_indent {
                    break;
                }
                let next_trimmed = next_line.trim_start();
                if next_trimmed.starts_with("- ") {
                    break;
                }
                if let Some(c_pos) = find_unquoted_colon(next_trimmed) {
                    let k = next_trimmed[..c_pos]
                        .trim()
                        .trim_matches(|c| c == '\'' || c == '"');
                    let r = next_trimmed[c_pos + 1..].trim();
                    *idx += 1;
                    map.insert(k.to_string(), parse_yaml_scalar(r));
                } else {
                    break;
                }
            }
            list.push(Value::Object(map));
        } else {
            list.push(parse_yaml_scalar(item_content));
        }
    }

    Ok(Value::Array(list))
}

fn parse_yaml_scalar(s: &str) -> Value {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed == "~" || trimmed == "null" {
        return Value::Null;
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        if trimmed.len() >= 2 {
            return Value::String(trimmed[1..trimmed.len() - 1].to_string());
        }
    }
    if let Ok(int_val) = trimmed.parse::<i64>() {
        return json!(int_val);
    }
    if let Ok(float_val) = trimmed.parse::<f64>() {
        if float_val.is_finite() {
            return json!(float_val);
        }
    }
    // Handle inline json array/object if present
    if (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('{') && trimmed.ends_with('}'))
    {
        if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
            return parsed;
        }
    }

    Value::String(trimmed.to_string())
}

fn line_indent(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

fn skip_blank_and_comments(lines: &[&str], idx: &mut usize) {
    while *idx < lines.len() {
        let trimmed = lines[*idx].trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            *idx += 1;
        } else {
            break;
        }
    }
}

fn find_unquoted_colon(s: &str) -> Option<usize> {
    let mut in_single = false;
    let mut in_double = false;
    for (i, c) in s.char_indices() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ':' if !in_single && !in_double => {
                // Ensure colon is followed by whitespace, end-of-string, or newline
                let next = s[i + 1..].chars().next();
                if next.is_none() || next == Some(' ') || next == Some('\t') {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

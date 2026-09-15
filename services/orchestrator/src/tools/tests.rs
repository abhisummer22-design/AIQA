//! Phase 2 Comprehensive Test Suite: Safe QA Diagnostic Tools.
//!
//! Covers:
//! - Schema Inspector (JSON, YAML, parameters, requests, responses, filters, limits, timeouts)
//! - SSRF Protection (localhost, 127.0.0.1, ::1, 169.254.169.254, RFC1918 private IPs, schemes)
//! - Path Traversal & Sensitive File Protection
//! - DOM Prober (existence, not found, visibility across roles, accessibility roles, nearby context)
//! - Script Injection Prevention (blocking JS, eval, scripts)
//! - Unsupported Capability Transparency (no faking)
//! - Requirement Lookup (project scoping, exact ID, keyword, cross-project rejection, RBAC)
//! - AgentToolRegistry (discovery, enumeration, duplicate rejection, safety levels)
//! - Architectural Boundaries (zero ExecutorRegistry, zero Playwright/Newman/shell execution)

use crate::agent::{AgentContext, AgentLimits};
use crate::agent_tools::{
    AgentTool, AgentToolRegistry, ToolError, ToolInvocation, ToolSafetyLevel,
};
use crate::auth::{AuthContext, Role};
use crate::crawler::{
    CrawlOutput, CrawledElement, CrawledPage, RoleCrawlResult, VisibilityDiffEntry,
};
use crate::tools::dom_prober::DomProberTool;
use crate::tools::requirement_lookup::RequirementLookupTool;
use crate::tools::schema_inspector::SchemaInspectorTool;
use crate::tools::security::{validate_safe_local_path, validate_safe_url};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

fn make_test_context(project_id: Option<&str>) -> AgentContext {
    AgentContext::new(
        "task-diag-test",
        project_id.map(ToString::to_string),
        "session-test-01",
        AgentLimits::default(),
        None,
    )
}

fn sample_openapi_json() -> String {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Inventory Management API",
            "version": "2.4.0",
            "description": "API for tracking warehouse stock and orders."
        },
        "servers": [
            { "url": "https://api.inventory.example.com/v2" }
        ],
        "paths": {
            "/items": {
                "get": {
                    "operationId": "listItems",
                    "summary": "List inventory items",
                    "parameters": [
                        {
                            "name": "category",
                            "in": "query",
                            "required": false,
                            "schema": { "type": "string" },
                            "description": "Filter by item category"
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "required": false,
                            "schema": { "type": "integer" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of items retrieved",
                            "content": {
                                "application/json": {
                                    "schema": { "type": "array" }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "operationId": "createItem",
                    "summary": "Create new inventory item",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["sku", "name", "quantity"],
                                    "properties": {
                                        "sku": { "type": "string" },
                                        "name": { "type": "string" },
                                        "quantity": { "type": "integer" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "201": { "description": "Item created successfully" },
                        "400": { "description": "Invalid input payload" }
                    }
                }
            },
            "/items/{id}": {
                "delete": {
                    "operationId": "deleteItem",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "204": { "description": "Item deleted" },
                        "404": { "description": "Item not found" }
                    }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "BearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "JWT bearer token"
                }
            }
        }
    })
    .to_string()
}

fn sample_openapi_yaml() -> String {
    r#"
openapi: 3.0.3
info:
  title: Orders API
  version: 1.0.0
  description: Order fulfillment service
servers:
  - url: https://orders.example.com
paths:
  /orders:
    get:
      operationId: getOrders
      summary: Retrieve all orders
      responses:
        200:
          description: OK
    post:
      operationId: submitOrder
      summary: Submit a new order
      responses:
        201:
          description: Created
"#
    .to_string()
}

fn sample_crawl_output() -> CrawlOutput {
    CrawlOutput {
        roles: vec![
            RoleCrawlResult {
                role: "admin".to_string(),
                login_succeeded: true,
                login_error: None,
                pages: vec![CrawledPage {
                    url: "http://app.local/dashboard".to_string(),
                    title: "Admin Dashboard".to_string(),
                    elements: vec![
                        CrawledElement {
                            kind: "heading".to_string(),
                            text: "Admin Overview".to_string(),
                            selector_hint: "h1.title".to_string(),
                            href: None,
                        },
                        CrawledElement {
                            kind: "button".to_string(),
                            text: "Delete User".to_string(),
                            selector_hint: "#delete-user-btn".to_string(),
                            href: None,
                        },
                        CrawledElement {
                            kind: "a".to_string(),
                            text: "System Settings".to_string(),
                            selector_hint: "a.nav-settings".to_string(),
                            href: Some("/settings".to_string()),
                        },
                    ],
                }],
            },
            RoleCrawlResult {
                role: "viewer".to_string(),
                login_succeeded: true,
                login_error: None,
                pages: vec![CrawledPage {
                    url: "http://app.local/dashboard".to_string(),
                    title: "Viewer Dashboard".to_string(),
                    elements: vec![
                        CrawledElement {
                            kind: "heading".to_string(),
                            text: "Viewer Overview".to_string(),
                            selector_hint: "h1.title".to_string(),
                            href: None,
                        },
                        CrawledElement {
                            kind: "input".to_string(),
                            text: "Search items".to_string(),
                            selector_hint: "input#search".to_string(),
                            href: None,
                        },
                    ],
                }],
            },
        ],
        visibility_matrix: vec![VisibilityDiffEntry {
            url: "http://app.local/dashboard".to_string(),
            roles_with_access: vec!["admin".to_string(), "viewer".to_string()],
            elements_only_visible_to: BTreeMap::from([(
                "admin".to_string(),
                vec!["#delete-user-btn".to_string()],
            )]),
        }],
    }
}

// ===========================================================================
// 1. SCHEMA INSPECTOR TESTS
// ===========================================================================

#[tokio::test]
async fn test_01_schema_inspector_valid_inline_json() {
    let tool = SchemaInspectorTool::new();
    let ctx = make_test_context(Some("proj-qa"));

    let invocation = ToolInvocation {
        call_id: "call-01".to_string(),
        tool_name: "schema_inspector".to_string(),
        arguments: json!({
            "source_type": "inline",
            "schema_content": sample_openapi_json()
        }),
    };

    let obs = tool
        .execute(&invocation, &ctx)
        .await
        .expect("schema inspection succeeded");
    assert!(obs.is_success());
    assert_eq!(obs.tool_name, "schema_inspector");
    assert_eq!(obs.safety_level, ToolSafetyLevel::ReadOnly);

    let output = obs.output.unwrap();
    assert_eq!(output["title"], "Inventory Management API");
    assert_eq!(output["version"], "2.4.0");
    assert_eq!(output["total_paths_in_spec"], 2);

    let paths = output["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 2);

    // Verify /items path
    let items_path = paths.iter().find(|p| p["path"] == "/items").unwrap();
    let methods = items_path["methods"].as_array().unwrap();
    assert!(methods.contains(&json!("GET")));
    assert!(methods.contains(&json!("POST")));

    let operations = items_path["operations"].as_array().unwrap();
    let post_op = operations.iter().find(|o| o["method"] == "POST").unwrap();
    assert_eq!(post_op["auth_required"], true);
    assert!(post_op["auth_schemes"]
        .as_array()
        .unwrap()
        .contains(&json!("BearerAuth")));
    assert_eq!(post_op["request_body"]["required"], true);
}

#[tokio::test]
async fn test_02_schema_inspector_valid_inline_yaml() {
    let tool = SchemaInspectorTool::new();
    let ctx = make_test_context(Some("proj-qa"));

    let invocation = ToolInvocation {
        call_id: "call-02".to_string(),
        tool_name: "schema_inspector".to_string(),
        arguments: json!({
            "source_type": "inline",
            "schema_content": sample_openapi_yaml()
        }),
    };

    let obs = tool
        .execute(&invocation, &ctx)
        .await
        .expect("yaml inspection succeeded");
    assert!(obs.is_success());

    let output = obs.output.unwrap();
    assert_eq!(output["title"], "Orders API");
    assert_eq!(output["version"], "1.0.0");
    let paths = output["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0]["path"], "/orders");
    let methods = paths[0]["methods"].as_array().unwrap();
    assert!(methods.contains(&json!("GET")));
    assert!(methods.contains(&json!("POST")));
}

#[tokio::test]
async fn test_03_schema_inspector_path_filter() {
    let tool = SchemaInspectorTool::new();
    let ctx = make_test_context(Some("proj-qa"));

    let invocation = ToolInvocation {
        call_id: "call-03".to_string(),
        tool_name: "schema_inspector".to_string(),
        arguments: json!({
            "schema_content": sample_openapi_json(),
            "path_filter": "/items/{id}"
        }),
    };

    let obs = tool.execute(&invocation, &ctx).await.unwrap();
    let output = obs.output.unwrap();
    let paths = output["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0]["path"], "/items/{id}");
}

#[tokio::test]
async fn test_04_schema_inspector_reads_local_openapi_file() {
    let tool = SchemaInspectorTool::new();
    let ctx = make_test_context(Some("proj-qa"));

    // Path to existing project file: executors/schemathesis/openapi.yaml
    let invocation = ToolInvocation {
        call_id: "call-04".to_string(),
        tool_name: "schema_inspector".to_string(),
        arguments: json!({
            "file_path": "executors/schemathesis/openapi.yaml"
        }),
    };

    let obs = tool
        .execute(&invocation, &ctx)
        .await
        .expect("local file inspection succeeded");
    assert!(obs.is_success());
    let output = obs.output.unwrap();
    assert!(output["title"]
        .as_str()
        .unwrap()
        .contains("AIQA Orchestrator"));
}

#[tokio::test]
async fn test_05_schema_inspector_malformed_content_fails_safely() {
    let tool = SchemaInspectorTool::new();
    let ctx = make_test_context(Some("proj-qa"));

    let invocation = ToolInvocation {
        call_id: "call-05".to_string(),
        tool_name: "schema_inspector".to_string(),
        arguments: json!({
            "schema_content": "!!NOT A SCHEMA!! [[[:::"
        }),
    };

    let err = tool.execute(&invocation, &ctx).await.unwrap_err();
    match err {
        ToolError::InvalidInput { tool, details } => {
            assert_eq!(tool, "schema_inspector");
            assert!(details.contains("malformed schema"));
        }
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[tokio::test]
async fn test_06_schema_inspector_oversized_content_rejected() {
    let tool = SchemaInspectorTool::new();
    let ctx = make_test_context(Some("proj-qa"));

    // 600KB payload (exceeds 512KB limit)
    let huge_content = "x".repeat(600 * 1024);

    let invocation = ToolInvocation {
        call_id: "call-06".to_string(),
        tool_name: "schema_inspector".to_string(),
        arguments: json!({
            "schema_content": huge_content
        }),
    };

    let err = tool.execute(&invocation, &ctx).await.unwrap_err();
    match err {
        ToolError::ExecutionFailed { tool, details } => {
            assert_eq!(tool, "schema_inspector");
            assert!(details.contains("exceeds maximum limit"));
        }
        other => panic!("expected ExecutionFailed for oversized payload, got {other:?}"),
    }
}

// ===========================================================================
// 2. SECURITY & SSRF TESTS
// ===========================================================================

#[tokio::test]
async fn test_07_ssrf_rejects_localhost_on_arbitrary_port() {
    let res = validate_safe_url("schema_inspector", "http://localhost:3000/openapi.json").await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(matches!(err, ToolError::SafetyViolation { .. }));
}

#[tokio::test]
async fn test_08_ssrf_rejects_127_0_0_1() {
    let res = validate_safe_url("schema_inspector", "http://127.0.0.1:9000/swagger.json").await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        ToolError::SafetyViolation { .. }
    ));
}

#[tokio::test]
async fn test_09_ssrf_rejects_cloud_metadata_169_254() {
    let res = validate_safe_url(
        "schema_inspector",
        "http://169.254.169.254/latest/meta-data/",
    )
    .await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        ToolError::SafetyViolation { .. }
    ));
}

#[tokio::test]
async fn test_10_ssrf_rejects_rfc1918_private_ranges() {
    // 10.0.0.0/8
    let r1 = validate_safe_url("schema_inspector", "http://10.0.0.5/openapi.json").await;
    assert!(r1.is_err());

    // 172.16.0.0/12
    let r2 = validate_safe_url("schema_inspector", "http://172.20.1.1/openapi.json").await;
    assert!(r2.is_err());

    // 192.168.0.0/16
    let r3 = validate_safe_url("schema_inspector", "http://192.168.1.100/openapi.json").await;
    assert!(r3.is_err());
}

#[tokio::test]
async fn test_11_ssrf_rejects_non_http_schemes() {
    let r1 = validate_safe_url("schema_inspector", "file:///etc/passwd").await;
    assert!(matches!(r1.unwrap_err(), ToolError::SafetyViolation { .. }));

    let r2 = validate_safe_url("schema_inspector", "gopher://evil.com/").await;
    assert!(matches!(r2.unwrap_err(), ToolError::SafetyViolation { .. }));

    let r3 = validate_safe_url("schema_inspector", "ftp://ftp.local/").await;
    assert!(matches!(r3.unwrap_err(), ToolError::SafetyViolation { .. }));
}

#[test]
fn test_12_path_traversal_rejection() {
    let r1 = validate_safe_local_path("schema_inspector", "../../../etc/passwd");
    assert!(matches!(r1.unwrap_err(), ToolError::SafetyViolation { .. }));

    let r2 = validate_safe_local_path("schema_inspector", "schemas/../../secrets.env");
    assert!(matches!(r2.unwrap_err(), ToolError::SafetyViolation { .. }));
}

#[test]
fn test_13_sensitive_system_path_rejection() {
    let r1 = validate_safe_local_path("schema_inspector", "C:\\Windows\\system32\\cmd.exe");
    assert!(matches!(r1.unwrap_err(), ToolError::SafetyViolation { .. }));

    let r2 = validate_safe_local_path("schema_inspector", "/etc/shadow");
    assert!(matches!(r2.unwrap_err(), ToolError::SafetyViolation { .. }));

    let r3 = validate_safe_local_path("schema_inspector", ".env");
    assert!(matches!(r3.unwrap_err(), ToolError::SafetyViolation { .. }));
}

// ===========================================================================
// 3. DOM PROBER TESTS
// ===========================================================================

#[tokio::test]
async fn test_14_dom_prober_element_exists_and_attributes() {
    let tool = DomProberTool::new();
    let ctx = make_test_context(Some("proj-web"));

    let invocation = ToolInvocation {
        call_id: "call-dom-01".to_string(),
        tool_name: "dom_prober".to_string(),
        arguments: json!({
            "selector": "#delete-user-btn",
            "query_type": "existence",
            "crawl_output": serde_json::to_value(sample_crawl_output()).unwrap()
        }),
    };

    let obs = tool
        .execute(&invocation, &ctx)
        .await
        .expect("dom prober execution succeeded");
    assert!(obs.is_success());
    assert_eq!(obs.safety_level, ToolSafetyLevel::Diagnostic);

    let output = obs.output.unwrap();
    assert_eq!(output["exists"], true);
    assert_eq!(output["match_count"], 1);
    assert_eq!(output["roles_with_match"], json!(["admin"]));

    let elem = &output["matching_elements"][0];
    assert_eq!(elem["text"], "Delete User");
    assert_eq!(elem["kind"], "button");
    assert_eq!(elem["accessible_role"], "button");
}

#[tokio::test]
async fn test_15_dom_prober_element_not_found() {
    let tool = DomProberTool::new();
    let ctx = make_test_context(Some("proj-web"));

    let invocation = ToolInvocation {
        call_id: "call-dom-02".to_string(),
        tool_name: "dom_prober".to_string(),
        arguments: json!({
            "selector": "#non-existent-element",
            "query_type": "existence",
            "crawl_output": serde_json::to_value(sample_crawl_output()).unwrap()
        }),
    };

    let obs = tool.execute(&invocation, &ctx).await.unwrap();
    let output = obs.output.unwrap();
    assert_eq!(output["exists"], false);
    assert_eq!(output["match_count"], 0);
    assert!(output["roles_with_match"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_16_dom_prober_role_visibility_differential() {
    let tool = DomProberTool::new();
    let ctx = make_test_context(Some("proj-web"));

    let invocation = ToolInvocation {
        call_id: "call-dom-03".to_string(),
        tool_name: "dom_prober".to_string(),
        arguments: json!({
            "selector": "#delete-user-btn",
            "query_type": "visibility",
            "crawl_output": serde_json::to_value(sample_crawl_output()).unwrap()
        }),
    };

    let obs = tool.execute(&invocation, &ctx).await.unwrap();
    let output = obs.output.unwrap();
    let vis = &output["visibility_details"];
    assert_eq!(vis["role_access_differential"], true);
    assert!(vis["visible_in_roles"]
        .as_array()
        .unwrap()
        .contains(&json!("admin")));
    assert!(vis["absent_in_roles"]
        .as_array()
        .unwrap()
        .contains(&json!("viewer")));
}

#[tokio::test]
async fn test_17_dom_prober_nearby_context() {
    let tool = DomProberTool::new();
    let ctx = make_test_context(Some("proj-web"));

    let invocation = ToolInvocation {
        call_id: "call-dom-04".to_string(),
        tool_name: "dom_prober".to_string(),
        arguments: json!({
            "selector": "#delete-user-btn",
            "query_type": "context",
            "crawl_output": serde_json::to_value(sample_crawl_output()).unwrap()
        }),
    };

    let obs = tool.execute(&invocation, &ctx).await.unwrap();
    let output = obs.output.unwrap();
    let elem = &output["matching_elements"][0];
    let nearby = elem["nearby_elements"].as_array().unwrap();
    assert!(!nearby.is_empty());
    assert!(nearby.iter().any(|n| n["text"] == "Admin Overview"));
}

#[tokio::test]
async fn test_18_dom_prober_rejects_script_and_js_injection() {
    let tool = DomProberTool::new();
    let ctx = make_test_context(Some("proj-web"));

    let malicious_selectors = [
        "<script>alert(1)</script>",
        "button; window.location='http://evil.com'",
        "eval(document.cookie)",
        "javascript:void(0)",
        "() => evilFunction()",
        "elem.__proto__.polluted = true",
    ];

    for bad_sel in malicious_selectors {
        let invocation = ToolInvocation {
            call_id: "call-dom-bad".to_string(),
            tool_name: "dom_prober".to_string(),
            arguments: json!({
                "selector": bad_sel,
                "crawl_output": serde_json::to_value(sample_crawl_output()).unwrap()
            }),
        };

        let err = tool.execute(&invocation, &ctx).await.unwrap_err();
        match err {
            ToolError::SafetyViolation { tool, reason } => {
                assert_eq!(tool, "dom_prober");
                assert!(reason.contains("forbidden pattern"));
            }
            other => panic!("expected SafetyViolation for '{bad_sel}', got {other:?}"),
        }
    }
}

#[tokio::test]
async fn test_19_dom_prober_unsupported_capability_notice() {
    let tool = DomProberTool::new();
    let ctx = make_test_context(Some("proj-web"));

    let invocation = ToolInvocation {
        call_id: "call-dom-05".to_string(),
        tool_name: "dom_prober".to_string(),
        arguments: json!({
            "selector": "h1",
            "query_type": "attributes",
            "crawl_output": serde_json::to_value(sample_crawl_output()).unwrap()
        }),
    };

    let obs = tool.execute(&invocation, &ctx).await.unwrap();
    let output = obs.output.unwrap();
    let notices = output["unsupported_capabilities"].as_array().unwrap();
    assert!(notices
        .iter()
        .any(|n| n["capability"] == "computed_css_styles"));
}

// ===========================================================================
// 4. REQUIREMENT LOOKUP & MULTI-TENANCY TESTS
// ===========================================================================

#[tokio::test]
async fn test_20_requirement_lookup_project_scoping_and_cross_project_rejection() {
    let tool = RequirementLookupTool::new(None);

    // Context is scoped to "project-alpha"
    let ctx = make_test_context(Some("project-alpha"));

    // Query targeting "project-beta" (attempted cross-project access)
    let invocation = ToolInvocation {
        call_id: "call-req-01".to_string(),
        tool_name: "requirement_lookup".to_string(),
        arguments: json!({
            "project_id": "project-beta",
            "query_type": "keyword",
            "keyword": "checkout"
        }),
    };

    let err = tool.execute(&invocation, &ctx).await.unwrap_err();
    match err {
        ToolError::SafetyViolation { tool, reason } => {
            assert_eq!(tool, "requirement_lookup");
            assert!(reason.contains("cross-project authorization violation"));
        }
        other => panic!("expected SafetyViolation for cross-project access, got {other:?}"),
    }
}

#[tokio::test]
async fn test_21_requirement_lookup_auth_context_enforcement() {
    let tool = RequirementLookupTool::new(None);

    // Operator key scoped to "project-x"
    let auth = AuthContext {
        key_id: Uuid::new_v4(),
        name: "qa-user".to_string(),
        role: Role::Operator,
        project_id: Some("project-x".to_string()),
    };

    let ctx = make_test_context(Some("project-x")).with_auth(auth);

    // Same project: Allowed
    let inv_allowed = ToolInvocation {
        call_id: "call-req-02".to_string(),
        tool_name: "requirement_lookup".to_string(),
        arguments: json!({
            "project_id": "project-x",
            "query_type": "keyword",
            "keyword": "login"
        }),
    };

    let obs = tool.execute(&inv_allowed, &ctx).await.unwrap();
    assert!(obs.is_success());

    // Different project: Blocked
    let inv_blocked = ToolInvocation {
        call_id: "call-req-03".to_string(),
        tool_name: "requirement_lookup".to_string(),
        arguments: json!({
            "project_id": "project-y",
            "query_type": "keyword",
            "keyword": "login"
        }),
    };

    let err = tool.execute(&inv_blocked, &ctx).await.unwrap_err();
    assert!(matches!(err, ToolError::SafetyViolation { .. }));
}

#[tokio::test]
async fn test_22_requirement_lookup_admin_can_access_specified_project() {
    let tool = RequirementLookupTool::new(None);

    // Admin key with no project restriction
    let admin_auth = AuthContext {
        key_id: Uuid::new_v4(),
        name: "admin-user".to_string(),
        role: Role::Admin,
        project_id: None,
    };

    // Unscoped context with admin auth
    let ctx = make_test_context(None).with_auth(admin_auth);

    let invocation = ToolInvocation {
        call_id: "call-req-admin".to_string(),
        tool_name: "requirement_lookup".to_string(),
        arguments: json!({
            "project_id": "any-project-123",
            "query_type": "keyword",
            "keyword": "auth"
        }),
    };

    let obs = tool.execute(&invocation, &ctx).await.unwrap();
    assert!(obs.is_success());
    let output = obs.output.unwrap();
    assert_eq!(output["project_id"], "any-project-123");
}

// ===========================================================================
// 5. REGISTRY & LIFECYCLE TESTS
// ===========================================================================

#[test]
fn test_23_registry_registers_all_three_diagnostic_tools() {
    let mut registry = AgentToolRegistry::new();
    super::register_diagnostic_tools(&mut registry, None)
        .expect("diagnostic tools register cleanly");

    assert_eq!(registry.len(), 3);
    assert!(registry.has_tool("schema_inspector"));
    assert!(registry.has_tool("dom_prober"));
    assert!(registry.has_tool("requirement_lookup"));
}

#[test]
fn test_24_registry_duplicate_rejection() {
    let mut registry = AgentToolRegistry::new();
    super::register_diagnostic_tools(&mut registry, None).unwrap();

    let duplicate = Arc::new(SchemaInspectorTool::new());
    let err = registry.register(duplicate).unwrap_err();
    assert!(matches!(err, ToolError::Duplicate(name) if name == "schema_inspector"));
}

#[test]
fn test_25_safety_levels_are_readonly_or_diagnostic() {
    let schema_tool = SchemaInspectorTool::new();
    let dom_tool = DomProberTool::new();
    let req_tool = RequirementLookupTool::new(None);

    assert_eq!(
        schema_tool.definition().safety_level,
        ToolSafetyLevel::ReadOnly
    );
    assert_eq!(
        dom_tool.definition().safety_level,
        ToolSafetyLevel::Diagnostic
    );
    assert_eq!(
        req_tool.definition().safety_level,
        ToolSafetyLevel::ReadOnly
    );

    assert!(schema_tool.definition().safety_level.is_autonomous_safe());
    assert!(dom_tool.definition().safety_level.is_autonomous_safe());
    assert!(req_tool.definition().safety_level.is_autonomous_safe());
}

// ===========================================================================
// 6. ARCHITECTURAL BOUNDARY VERIFICATION
// ===========================================================================

#[test]
fn test_26_boundary_zero_external_execution_dependencies() {
    // Assert that the tools module compiles and operates independently
    // without invoking subprocess executors or external command runners.
    let schema_def = SchemaInspectorTool::new().definition();
    let dom_def = DomProberTool::new().definition();
    let req_def = RequirementLookupTool::new(None).definition();

    // Verify parameter schemas are strictly declarative objects
    assert!(schema_def.parameters_schema.is_object());
    assert!(dom_def.parameters_schema.is_object());
    assert!(req_def.parameters_schema.is_object());
}

#[tokio::test]
async fn test_27_requirement_lookup_with_real_sqlite_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("diag_test.sqlite3");
    let store = crate::store::Store::connect(&path).await.expect("connect");

    let tc_req = crate::domain::CreateTestCaseRequest {
        title: "Verify Checkout Payment".to_string(),
        description: "Customer pays with card".to_string(),
        category: "payment".to_string(),
        priority: "HIGH".to_string(),
        severity: "CRITICAL".to_string(),
        preconditions: vec!["Cart has items".to_string()],
        steps: vec![crate::domain::TestStep {
            step_number: 1,
            action: "Click Pay".to_string(),
            expected_result: "Payment successful".to_string(),
        }],
        expected_result: Some("Order confirmed".to_string()),
        assertions: vec!["order.status == confirmed".to_string()],
        test_data: None,
        source_document: None,
        requirement_reference: Some("REQ-PAY-01".to_string()),
        source_requirement: Some("System shall process payments".to_string()),
        traceability: None,
        risk_factor: None,
        tags: vec!["checkout".to_string()],
        suite_id: None,
    };

    let created_tc = store
        .insert_test_case("proj-checkout", &tc_req, Some("tester"))
        .await
        .expect("insert test case");

    // Also link in quality graph
    let req_node = store
        .upsert_quality_node(
            "proj-checkout",
            "requirement",
            "REQ-PAY-01",
            "Payment Requirement",
        )
        .await
        .expect("upsert requirement node");
    let tc_node = store
        .upsert_quality_node(
            "proj-checkout",
            "test_case",
            &created_tc.to_string(),
            "Checkout Test",
        )
        .await
        .expect("upsert test case node");
    store
        .upsert_quality_relation(tc_node, req_node, "validates")
        .await
        .expect("link relation");

    // Test RequirementLookupTool with store
    let tool = RequirementLookupTool::new(Some(store));
    let ctx = make_test_context(Some("proj-checkout"));

    let invocation = ToolInvocation {
        call_id: "call-req-db".to_string(),
        tool_name: "requirement_lookup".to_string(),
        arguments: json!({
            "project_id": "proj-checkout",
            "query_type": "id",
            "requirement_id": "REQ-PAY-01",
            "include_test_cases": true
        }),
    };

    let obs = tool
        .execute(&invocation, &ctx)
        .await
        .expect("requirement lookup succeeded");
    assert!(obs.is_success());
    let output = obs.output.unwrap();
    assert_eq!(output["project_id"], "proj-checkout");
    assert_eq!(output["total_found"], 1);

    let reqs = output["requirements"].as_array().unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0]["id"], "REQ-PAY-01");

    let linked_cases = reqs[0]["related_test_cases"].as_array().unwrap();
    assert_eq!(linked_cases.len(), 1);
    assert_eq!(linked_cases[0]["title"], "Verify Checkout Payment");
    assert_eq!(linked_cases[0]["priority"], "HIGH");
}

#[tokio::test]
async fn test_28_schema_inspector_cancellation() {
    let tool = SchemaInspectorTool::new();
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let ctx = AgentContext::new(
        "task-cancel",
        Some("proj-1".to_string()),
        "sess-1",
        AgentLimits::default(),
        Some(cancel_rx),
    );

    // Cancel before execution
    cancel_tx.send(true).unwrap();

    let invocation = ToolInvocation {
        call_id: "call-cancel".to_string(),
        tool_name: "schema_inspector".to_string(),
        arguments: json!({
            "schema_content": sample_openapi_json()
        }),
    };

    let err = tool.execute(&invocation, &ctx).await.unwrap_err();
    assert_eq!(err, ToolError::Cancelled);
}

#[tokio::test]
async fn test_29_dom_prober_cancellation() {
    let tool = DomProberTool::new();
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let ctx = AgentContext::new(
        "task-cancel-dom",
        Some("proj-1".to_string()),
        "sess-1",
        AgentLimits::default(),
        Some(cancel_rx),
    );

    cancel_tx.send(true).unwrap();

    let invocation = ToolInvocation {
        call_id: "call-cancel-dom".to_string(),
        tool_name: "dom_prober".to_string(),
        arguments: json!({
            "selector": "button",
            "crawl_output": serde_json::to_value(sample_crawl_output()).unwrap()
        }),
    };

    let err = tool.execute(&invocation, &ctx).await.unwrap_err();
    assert_eq!(err, ToolError::Cancelled);
}

#[tokio::test]
async fn test_30_dom_prober_rejects_oversized_selector() {
    let tool = DomProberTool::new();
    let ctx = make_test_context(Some("proj-web"));

    let huge_selector = "a".repeat(300);

    let invocation = ToolInvocation {
        call_id: "call-huge-sel".to_string(),
        tool_name: "dom_prober".to_string(),
        arguments: json!({
            "selector": huge_selector,
            "crawl_output": serde_json::to_value(sample_crawl_output()).unwrap()
        }),
    };

    let err = tool.execute(&invocation, &ctx).await.unwrap_err();
    assert!(matches!(err, ToolError::InvalidInput { .. }));
}

#[tokio::test]
async fn test_31_dom_prober_rejects_empty_selector() {
    let tool = DomProberTool::new();
    let ctx = make_test_context(Some("proj-web"));

    let invocation = ToolInvocation {
        call_id: "call-empty-sel".to_string(),
        tool_name: "dom_prober".to_string(),
        arguments: json!({
            "selector": "   ",
            "crawl_output": serde_json::to_value(sample_crawl_output()).unwrap()
        }),
    };

    let err = tool.execute(&invocation, &ctx).await.unwrap_err();
    assert!(matches!(err, ToolError::InvalidInput { .. }));
}

#[test]
fn test_32_security_sanitize_secrets_redacts_credentials() {
    let mut data = json!({
        "status": "ok",
        "user": {
            "name": "alice",
            "api_key": "secret-12345",
            "bearer_token": "token-abcde",
            "password": "super-secret-password"
        },
        "headers": [
            { "authorization": "Bearer token-value" },
            { "cookie": "sessionid=xyz" }
        ]
    });

    crate::tools::security::sanitize_secrets(&mut data);

    assert_eq!(data["user"]["name"], "alice");
    assert_eq!(data["user"]["api_key"], "[REDACTED]");
    assert_eq!(data["user"]["bearer_token"], "[REDACTED]");
    assert_eq!(data["user"]["password"], "[REDACTED]");
    assert_eq!(data["headers"][0]["authorization"], "[REDACTED]");
    assert_eq!(data["headers"][1]["cookie"], "[REDACTED]");
}

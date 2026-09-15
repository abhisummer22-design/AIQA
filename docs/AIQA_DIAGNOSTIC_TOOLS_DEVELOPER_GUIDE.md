# AIQA Developer Guide: Phase 2 Safe QA Diagnostic Tools

This guide documents the native Rust diagnostic tools introduced in **Phase 2** of the AIQA architecture evolution. These tools build upon the Phase 1 `AgentTool` abstraction and provide safe, structured, read-only/diagnostic application inspection for AI agents before test plan generation or modification.

---

## 1. Architectural Role & Boundary

```text
               AI AGENT REASONING LOOP
                          │
                          ▼
                  AgentToolRegistry
                          │
         ┌────────────────┼────────────────┐
         ▼                ▼                ▼
   schema_inspector   dom_prober   requirement_lookup
   (OpenAPI/Swagger) (CrawlOutput)  (Quality Graph)
         │                │                │
         └────────────────┼────────────────┘
                          │
                          ▼
                 StructuredObservation
                          │
                          ▼
                  Agent Next Step
                          │
                          ▼
            Downstream Test Plan Compiler
                 (execution_plan.rs)
                          │
                          ▼
                  ExecutorRegistry
             (Playwright, Pytest, Newman)
```

### AgentTools vs Executors
| Aspect | AgentTools (`agent_tools.rs`, `tools/`) | Executors (`executor.rs`) |
| :--- | :--- | :--- |
| **Purpose** | Information gathering & pre-generation reasoning | End-to-end test execution & validation |
| **Safety Level** | `ReadOnly`, `Diagnostic` | `Mutating`, `Dangerous` |
| **Target App Impact** | Side-effect-free | Executes actions, fills forms, hits APIs |
| **Process Model** | In-process asynchronous Rust | Isolated OS subprocesses with Win32 Job Objects |
| **Network Guard** | SSRF protection, strict timeouts, no redirects | SSRF protection, job timeouts, process tree kill |
| **Persistence** | Operational telemetry only (no private thought leaks) | Writes stdout, stderr, screenshots, evidence |

---

## 2. Tool 1: `schema_inspector`

### Purpose
Safely parses and extracts structured metadata from an OpenAPI (3.0/3.1) or Swagger (2.0) API schema without running tests or fuzzers.

### Supported Sources
1. **Inline**: Raw JSON or YAML content provided in the arguments.
2. **Local File**: Path to local OpenAPI schema (e.g., `executors/schemathesis/openapi.yaml`).
3. **Remote URL**: HTTP/HTTPS URL serving an OpenAPI definition.

### Security & SSRF Protection
- Remote URLs are validated using `crate::executor::validate_target_url`.
- Blocked targets: `localhost` (unless port 8080 orchestrator exception), `127.0.0.0/8`, `::1`, RFC1918 private ranges (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`), cloud metadata (`169.254.169.254`), and non-HTTP schemes.
- HTTP redirects are blocked to prevent redirect-based SSRF pivoting.
- Local file paths are guarded against directory traversal (`..`) and sensitive system files (`/etc`, `C:\Windows`, `.env`).
- Maximum response size is capped at 512 KB.

### Input Schema
```json
{
  "type": "object",
  "properties": {
    "source_type": { "type": "string", "enum": ["inline", "file", "url"] },
    "schema_content": { "type": "string" },
    "file_path": { "type": "string" },
    "url": { "type": "string" },
    "path_filter": { "type": "string" },
    "max_paths": { "type": "integer" }
  }
}
```

### Output Schema
```json
{
  "title": "Inventory Management API",
  "version": "2.4.0",
  "servers": ["https://api.inventory.example.com/v2"],
  "total_paths_in_spec": 12,
  "returned_paths_count": 2,
  "is_truncated": false,
  "paths": [
    {
      "path": "/items",
      "methods": ["GET", "POST"],
      "operations": [
        {
          "method": "POST",
          "operation_id": "createItem",
          "summary": "Create new inventory item",
          "parameters": [],
          "request_body": {
            "required": true,
            "content_types": ["application/json"],
            "schema": { "type": "object" }
          },
          "responses": [
            { "status_code": "201", "description": "Item created" }
          ],
          "auth_required": true,
          "auth_schemes": ["BearerAuth"]
        }
      ]
    }
  ],
  "security_schemes": [
    { "name": "BearerAuth", "scheme_type": "http" }
  ]
}
```

---

## 3. Tool 2: `dom_prober`

### Purpose
Inspects existing static crawler outputs (`CrawlOutput`) to answer element existence, accessibility roles, cross-role visibility differentials, attributes, and surrounding DOM context without launching a browser or running scripts.

### Strict Non-Execution Boundary
- **Zero Browser Launching**: Operates exclusively over pre-computed `CrawlOutput`.
- **Zero Script Execution**: Rejects JavaScript expressions, `<script>` tags, and executable payloads.
- **Transparent Capabilities**: Explicitly discloses when dynamic attributes (such as computed CSS styles) require a live layout engine rather than faking data.

### Input Schema
```json
{
  "type": "object",
  "properties": {
    "selector": { "type": "string", "description": "CSS selector hint or text locator (max 256 chars)" },
    "query_type": { "type": "string", "enum": ["existence", "visibility", "role", "attributes", "context", "all"] },
    "crawl_task_id": { "type": "string" },
    "crawl_output": { "type": "object" },
    "role_filter": { "type": "string" },
    "url_filter": { "type": "string" },
    "max_results": { "type": "integer" }
  },
  "required": ["selector"]
}
```

### Output Schema
```json
{
  "selector": "#delete-user-btn",
  "query_type": "all",
  "exists": true,
  "match_count": 1,
  "roles_with_match": ["admin"],
  "matching_elements": [
    {
      "role": "admin",
      "page_url": "http://app.local/dashboard",
      "page_title": "Admin Dashboard",
      "element_index": 2,
      "kind": "button",
      "text": "Delete User",
      "selector_hint": "#delete-user-btn",
      "accessible_role": "button",
      "nearby_elements": [
        { "kind": "heading", "text": "Admin Overview", "selector_hint": "h1.title" }
      ]
    }
  ],
  "visibility_details": {
    "visible_in_roles": ["admin"],
    "absent_in_roles": ["viewer"],
    "role_access_differential": true
  },
  "unsupported_capabilities": [
    {
      "capability": "computed_css_styles",
      "reason": "computed styles require active browser layout engine; not captured in static crawler snapshots"
    }
  ]
}
```

---

## 4. Tool 3: `requirement_lookup`

### Purpose
Retrieves project requirements, linked test cases, and quality-graph relationships within authorized multi-tenant boundaries.

### Multi-Tenancy & Authorization
- **Project Boundary Enforcement**: Queries are strictly scoped to the target project. If `AgentContext.project_id` is set, callers cannot access a different project.
- **RBAC Governance**: Uses `AgentContext.auth` (`Role::Viewer`, `Role::Operator`, `Role::Admin`). Non-admin tokens are restricted to their assigned `project_id`.
- **Cross-Project Access Rejection**: Attempting cross-project access returns `ToolError::SafetyViolation`.

### Input Schema
```json
{
  "type": "object",
  "properties": {
    "project_id": { "type": "string" },
    "query_type": { "type": "string", "enum": ["id", "keyword", "all"] },
    "requirement_id": { "type": "string" },
    "keyword": { "type": "string" },
    "include_test_cases": { "type": "boolean" },
    "limit": { "type": "integer" }
  }
}
```

### Output Schema
```json
{
  "project_id": "proj-checkout",
  "query_type": "id",
  "total_found": 1,
  "is_truncated": false,
  "requirements": [
    {
      "id": "REQ-PAY-01",
      "reference": "REQ-PAY-01",
      "title": "Payment Requirement",
      "quality_node_id": "8f8b5774-6727-4c40-be58-39a067ffdbef",
      "related_test_cases": [
        {
          "id": "c1387d85-5b4d-4581-9b6a-93e18a0e28f1",
          "title": "Verify Checkout Payment",
          "status": "GENERATED",
          "priority": "HIGH",
          "category": "payment",
          "severity": "CRITICAL"
        }
      ]
    }
  ]
}
```

---

## 5. Security & Resource Safeguards Summary

1. **Safety Levels**: All Phase 2 tools are strictly `ToolSafetyLevel::ReadOnly` or `ToolSafetyLevel::Diagnostic`. Zero mutating or dangerous tools are exposed.
2. **SSRF Protection**: Reuses `crate::executor::validate_target_url` directly.
3. **Execution Limits**: Respects `AgentContext` deadlines, cancellation watch channels, and maximum observation bytes.
4. **Secret Sanitization**: All outputs are recursively scanned to redact tokens, passwords, cookies, and API keys.
5. **No Thought Leakage**: Turn records and observations never store or log intermediate private chain-of-thought monologue.

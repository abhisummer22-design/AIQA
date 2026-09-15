# AIQA Phase 2 Report: Safe QA Diagnostic Tools

- **Date**: 2026-09-13
- **Status**: Phase 2 Complete & Verified
- **Baseline**: Branch `Demo`, Commit `0a67da3`, Phase 1 Complete
- **Test Suite Verification**: **573 passed, 0 failed, 2 ignored** (521 baseline + 20 Phase 1 + 32 Phase 2)
- **Compiler Health**: `cargo check -p aiqa-orchestrator` passed with 0 errors

---

## 1. Executive Summary

Phase 2 implements the first production-quality set of **Safe QA Diagnostic Tools** on top of the Phase 1 `AgentTool` abstraction.

In strict adherence to the approved architecture roadmap and safety constraints:
- **Zero OpenJarvis Code or Dependencies**: No Python runtimes, no PyO3, no LiteLLM.
- **Zero Production Test Execution**: Phase 2 tools do not execute Playwright specs, Newman collections, Pytest suites, Maestro flows, or Schemathesis fuzzers. The downstream `ExecutorRegistry` remains unmodified and untouched.
- **Strict ReadOnly & Diagnostic Classifications**: Zero mutating or dangerous tools. Tools perform side-effect-free inspection to empower agent reasoning before test plan generation.
- **Full SSRF & Multi-Tenant Protection**: Direct reuse of `crate::executor::validate_target_url` blocking loopback, RFC1918 private ranges, cloud metadata endpoints, and non-HTTP schemes. Direct enforcement of project scoping and RBAC via `AgentContext`.
- **Zero Private Thought Leakage**: Observations and logs preserve only structured operational telemetry, strictly omitting private chain-of-thought monologue.

---

## 2. Files Created & Modified

### Files Created
1. `services/orchestrator/src/tools/mod.rs` (38 lines)
   - Diagnostic tools module declaration and `register_diagnostic_tools()` registry helper.
2. `services/orchestrator/src/tools/security.rs` (215 lines)
   - Reusable security utilities: SSRF URL validation, redirect-safe bounded HTTP GET, path traversal validation, and recursive secret redaction.
3. `services/orchestrator/src/tools/schema_inspector.rs` (542 lines)
   - `SchemaInspectorTool` implementing `AgentTool` (`ToolSafetyLevel::ReadOnly`).
   - Parses OpenAPI 3.0/3.1 and Swagger 2.0 schemas from inline JSON/YAML, local files, or remote URLs.
   - Built-in lightweight YAML parser; extracts paths, methods, parameters, requests, responses, and security schemes.
4. `services/orchestrator/src/tools/dom_prober.rs` (386 lines)
   - `DomProberTool` implementing `AgentTool` (`ToolSafetyLevel::Diagnostic`).
   - Static inspection of `CrawlOutput` without browser launch; checks existence, roles, cross-role visibility differentials, attributes, and surrounding DOM context.
   - Strict script-injection prevention.
5. `services/orchestrator/src/tools/requirement_lookup.rs` (452 lines)
   - `RequirementLookupTool` implementing `AgentTool` (`ToolSafetyLevel::ReadOnly`).
   - Scoped querying of project requirements, Quality Graph relationships (`validates` edges), and linked test cases.
   - Multi-tenant boundary enforcement and RBAC verification.
6. `services/orchestrator/src/tools/tests.rs` (992 lines)
   - 32 deterministic unit and security tests covering all Phase 2 scenarios.
7. `docs/AIQA_DIAGNOSTIC_TOOLS_DEVELOPER_GUIDE.md` (210 lines)
   - Concise developer guide covering contracts, security, and boundaries.

### Files Modified
1. `services/orchestrator/src/agent.rs`
   - Extended `AgentContext` with `auth: Option<AuthContext>` and `store: Option<Store>`.
   - Added `with_auth()`, `with_store()`, and `is_authorized_for_project()` verification helpers.
2. `services/orchestrator/src/lib.rs`
   - Registered `pub mod tools;`.

---

## 3. Tool Architectural Specifications

### 3.1 Tool 1: Schema Inspector (`schema_inspector`)
- **Safety Level**: `ToolSafetyLevel::ReadOnly` (autonomous-safe).
- **Default Timeout**: 10,000 ms.
- **Contract**:
  - Inputs: `source_type` (`inline` | `file` | `url`), `schema_content`, `file_path`, `url`, `path_filter`, `max_paths`.
  - Output: Structured JSON containing `title`, `version`, `servers`, `paths` (with HTTP methods, operation IDs, parameters, request body schemas, response schemas, and auth schemes), and `security_schemes`.
- **Parsing Strategy**: Automatic format detection. Attempts direct JSON parsing first; falls back to an in-process indentation-aware YAML parser. Rejects invalid non-OpenAPI structures missing `openapi`, `swagger`, or `paths`.

### 3.2 Tool 2: DOM Prober (`dom_prober`)
- **Safety Level**: `ToolSafetyLevel::Diagnostic` (autonomous-safe).
- **Default Timeout**: 5,000 ms.
- **Contract**:
  - Inputs: `selector` (CSS hint, ID, class, or text locator, max 256 chars), `query_type` (`existence` | `visibility` | `role` | `attributes` | `context` | `all`), `crawl_task_id`, `crawl_output`, `role_filter`, `url_filter`, `max_results`.
  - Output: Structured JSON containing `exists`, `match_count`, `roles_with_match`, `matching_elements` (with accessible roles, tags, text, and surrounding DOM elements), and `visibility_details`.
- **Non-Execution Invariant**: Operates strictly against static `CrawlOutput`. Never launches a browser, executes Playwright, evaluates JavaScript, or mutates DOM.

### 3.3 Tool 3: Requirement Lookup (`requirement_lookup`)
- **Safety Level**: `ToolSafetyLevel::ReadOnly` (autonomous-safe).
- **Default Timeout**: 10,000 ms.
- **Contract**:
  - Inputs: `project_id`, `query_type` (`id` | `keyword` | `all`), `requirement_id`, `keyword`, `include_test_cases`, `limit`.
  - Output: Structured JSON containing `project_id`, `total_found`, and `requirements` (with requirement IDs, references, titles, summaries, quality node IDs, and linked test case summaries).
- **Persistence Integration**: Direct integration with `Store` SQLite connection pool, querying `quality_nodes`, `quality_relations`, and `test_cases`.

---

## 4. Security Model & Safeguards

### 4.1 SSRF Protection
- Remote requests in `schema_inspector` pass through `crate::executor::validate_target_url`.
- Reusable SSRF guard checks DNS-resolved IP addresses against:
  - Loopback (`127.0.0.0/8`, `::1`), except explicit orchestrator port 8080.
  - Link-local and Cloud Metadata (`169.254.169.254`, `fe80::/10`).
  - Private RFC1918 ranges (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`).
  - Non-HTTP/HTTPS schemes (`file://`, `gopher://`, `ftp://`).
- HTTP redirects are disabled (`Policy::none()`) to prevent redirect-based SSRF bypassing.

### 4.2 Local Path Safety
- Rejects directory traversal attempts (`..`) and null bytes (`\0`).
- Rejects access to sensitive system paths (`/etc`, `C:\Windows`, `.env`, `.ssh`, `id_rsa`).
- Enforces strict file size limits (<= 512 KB).

### 4.3 Multi-Tenant Project Scoping & RBAC
- `AgentContext::is_authorized_for_project` enforces strict isolation:
  - If context is scoped to `proj-A`, queries for `proj-B` immediately fail with `ToolError::SafetyViolation`.
  - If authenticated with `Role::Operator` or `Role::Viewer`, queries outside the key's assigned project are blocked.
  - Unscoped queries require `Role::Admin`.

### 4.4 Secret Sanitization
- `sanitize_secrets` recursively scans all output JSON structures before returning `ToolObservation`.
- Keys matching `token`, `password`, `secret`, `authorization`, `api_key`, `cookie`, etc., are masked as `"[REDACTED]"`.

---

## 5. Test Verification Matrix (32 New Tests Passed)

| Category | Test Scenario | Verified Invariant | Status |
| :--- | :--- | :--- | :--- |
| **Schema** | `test_01_schema_inspector_valid_inline_json` | Complete OpenAPI 3.0 JSON schema parsing | **PASSED** |
| **Schema** | `test_02_schema_inspector_valid_inline_yaml` | OpenAPI YAML parsing & method extraction | **PASSED** |
| **Schema** | `test_03_schema_inspector_path_filter` | Substring path filtering (`/items/{id}`) | **PASSED** |
| **Schema** | `test_04_schema_inspector_reads_local_openapi_file` | Reading local `executors/schemathesis/openapi.yaml` | **PASSED** |
| **Schema** | `test_05_schema_inspector_malformed_content_fails_safely` | Rejection of non-OpenAPI structures | **PASSED** |
| **Schema** | `test_06_schema_inspector_oversized_content_rejected` | Rejection of > 512 KB payload | **PASSED** |
| **Schema** | `test_28_schema_inspector_cancellation` | Cooperative cancellation via Tokio watch | **PASSED** |
| **Security**| `test_07_ssrf_rejects_localhost_on_arbitrary_port` | Blocks `http://localhost:3000` | **PASSED** |
| **Security**| `test_08_ssrf_rejects_127_0_0_1` | Blocks `http://127.0.0.1:9000` | **PASSED** |
| **Security**| `test_09_ssrf_rejects_cloud_metadata_169_254` | Blocks `http://169.254.169.254/...` | **PASSED** |
| **Security**| `test_10_ssrf_rejects_rfc1918_private_ranges` | Blocks 10.x, 172.16.x, 192.168.x | **PASSED** |
| **Security**| `test_11_ssrf_rejects_non_http_schemes` | Blocks `file://`, `gopher://`, `ftp://` | **PASSED** |
| **Security**| `test_12_path_traversal_rejection` | Blocks `../../etc/passwd` | **PASSED** |
| **Security**| `test_13_sensitive_system_path_rejection` | Blocks `C:\Windows`, `/etc/shadow`, `.env` | **PASSED** |
| **Security**| `test_32_security_sanitize_secrets_redacts_credentials` | Recursive secret masking in output | **PASSED** |
| **DOM** | `test_14_dom_prober_element_exists_and_attributes` | Element discovery & accessible role | **PASSED** |
| **DOM** | `test_15_dom_prober_element_not_found` | Graceful non-existent selector handling | **PASSED** |
| **DOM** | `test_16_dom_prober_role_visibility_differential` | Cross-role visibility diff (admin vs viewer) | **PASSED** |
| **DOM** | `test_17_dom_prober_nearby_context` | Extraction of surrounding elements | **PASSED** |
| **DOM** | `test_18_dom_prober_rejects_script_and_js_injection` | Rejection of `<script>`, `eval`, `() =>` | **PASSED** |
| **DOM** | `test_19_dom_prober_unsupported_capability_notice` | Explicit disclosure of computed CSS styling limit | **PASSED** |
| **DOM** | `test_29_dom_prober_cancellation` | Cooperative cancellation handling | **PASSED** |
| **DOM** | `test_30_dom_prober_rejects_oversized_selector` | Rejection of > 256 char selector | **PASSED** |
| **DOM** | `test_31_dom_prober_rejects_empty_selector` | Rejection of blank selector | **PASSED** |
| **Req** | `test_20_requirement_lookup_project_scoping` | Rejection of cross-project queries | **PASSED** |
| **Req** | `test_21_requirement_lookup_auth_context_enforcement` | Operator token project isolation | **PASSED** |
| **Req** | `test_22_requirement_lookup_admin_can_access_specified`| Admin cross-project access allowance | **PASSED** |
| **Req** | `test_27_requirement_lookup_with_real_sqlite_store` | Exact ID & Quality Graph edge retrieval | **PASSED** |
| **Registry**| `test_23_registry_registers_all_three_diagnostic_tools` | Discovery of all 3 diagnostic tools | **PASSED** |
| **Registry**| `test_24_registry_duplicate_rejection` | Rejection of duplicate tool registration | **PASSED** |
| **Registry**| `test_25_safety_levels_are_readonly_or_diagnostic` | Autonomous-safe verification | **PASSED** |
| **Boundary**| `test_26_boundary_zero_external_execution_dependencies`| Declarative schemas & zero executor coupling | **PASSED** |

---

## 6. Backward Compatibility & System Health

1. **Entire Rust Test Suite**:
   `cargo test -p aiqa-orchestrator --lib`
   - **573 passed, 0 failed, 2 ignored** (100% pass rate).
   - Zero regressions across existing execution engine, scheduler, and AI router tests.
2. **Compilation**:
   `cargo check -p aiqa-orchestrator` passed with 0 errors.
3. **Formatting**:
   Applied standard Rust formatting to all Phase 2 files.
4. **Existing Systems**:
   - `ai_generation.rs` remains unchanged.
   - `ai_router.rs` remains unchanged.
   - `execution_plan.rs` remains unchanged.
   - `engine.rs` & `scheduler.rs` remain unchanged.
   - `executor.rs` remains unchanged.

---

## 7. Limitations of Phase 2

1. **Static DOM Inspection**: `dom_prober` operates on `CrawlOutput` snapshots and cannot inspect live DOM states that change after dynamic user interactions without running a new crawl run.
2. **OpenAPI Scope**: The lightweight YAML parser is designed for standard OpenAPI document structures and intentionally ignores advanced YAML tags or complex anchors.
3. **Diagnostic Tools Only**: These tools do not compile test plans or execute real test runners. Downstream execution is reserved for subsequent phases.

---

## 8. Recommended Phase 3

**Phase 3 Objective: Specialized QA Agents**
Implement concrete, multi-turn QA agents that leverage the Phase 1 Foundation and Phase 2 Diagnostic Tools:
1. **Test Planning Agent (`planning.rs`)**:
   - Uses `schema_inspector` and `requirement_lookup` to decompose complex APIs and feature requirements into prioritized test suites before generation.
2. **Self-Healing Agent (`healing.rs`)**:
   - Uses `dom_prober` to diagnose failed selector assertions from execution logs and propose updated selector hints without re-running entire test suites.

---

**Execution Halted**: In accordance with instructions, Phase 2 is complete and all work has halted. Awaiting User Approval before proceeding to Phase 3.

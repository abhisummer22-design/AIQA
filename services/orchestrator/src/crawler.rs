//! Phase 6 — real crawler + per-role visibility matrix.
//!
//! Scope note (read before extending this module): the architecture
//! brief's own "hybrid crawler strategy" has three steps — shallow crawl,
//! targeted deep crawl, per-role authenticated session. This module builds
//! the first and third for real, verified end-to-end against a real,
//! self-contained fixture app (`executors/playwright/tests/crawler.spec.ts`
//! and `executors/playwright/fixtures/role_gated_app.js`) — not the second:
//! targeted deep crawling needs a requirement graph rich enough to say
//! which areas of a real app are "important," and `intelligence.rs`'s own
//! requirement-graph nodes (Phase 4) are flat identity-only records with no
//! such signal yet. Building targeting logic ahead of that signal existing
//! would be guessing, not targeting.
//!
//! Also deliberately NOT built here: a doc-vs-reality gap check. That needs
//! a real, structured "what the requirement documents say" side to diff
//! this module's real "what the app actually shows" side against — this
//! codebase has never had real document ingestion (PRD/FRD text
//! extraction), so that other side of the diff does not exist. Attempting
//! the gap check now would mean fabricating one half of a comparison.
//!
//! ```text
//! CrawlPlan (real base_url + real per-role login sequences)
//!         v
//! validate_crawl_plan()      -- SSRF guard (reuses executor::
//!                                validate_target_url, does not reinvent
//!                                it) + login-action-vocabulary check
//!         v
//! materialize_crawl_spec()   -- a real Playwright spec, executed through
//!                                the EXISTING PlaywrightExecutor/engine/
//!                                scheduler pipeline — no new subprocess
//!                                machinery
//!         v
//! (real browser crawl runs — executors/playwright/helpers/crawler.ts)
//!         v
//! read_crawl_result()        -- parses the real, structured JSON that run
//!                                wrote back (CrawlOutput: real per-role
//!                                pages reached + a real visibility matrix)
//! ```
//!
//! Security posture, same pattern `execution_plan.rs`'s own doc comment
//! already established: every `base_url` passes through the exact same
//! `executor::validate_target_url` SSRF guard real executors already use.
//! Login-sequence values (which may be real passwords, supplied by the
//! caller to authenticate as a real role) are embedded into the generated
//! spec file the same way `execution_plan::materialize_playwright_spec`
//! already embeds any compiled FILL action's value — real, existing at-rest
//! exposure this module does not newly introduce (that directory is
//! gitignored, but is not otherwise access-controlled; flagged here as an
//! honest limitation, not solved here — the same limitation the brief's own
//! §12 security posture section never covered either).

use crate::execution_plan::Action;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ------------------------------------------------------------------ schema

/// One role's real crawl configuration. `login_actions` reuses
/// `execution_plan::Action` — the exact same closed, safe vocabulary a
/// compiled test case's browser actions already use — rather than
/// inventing a second one for login steps specifically; `validate_crawl_plan`
/// restricts it to the three variants that make sense mid-login
/// (Navigate/Fill/Click). Empty `login_actions` crawls anonymously.
///
/// Snake_case on the wire (Rust's own default, no `rename_all`) —
/// matching every other real request/response type in this codebase
/// (`CreateTestCaseRequest`, `CreateTestPlanRequest`, ...), for both the
/// real HTTP request body (`api.rs::start_crawl_scoped`) and this module's
/// own tests. The generated Playwright spec's embedded plan needs
/// camelCase instead, to match `helpers/crawler.ts`'s own TypeScript
/// convention — that conversion happens once, explicitly, in
/// `plan_to_camel_case_json` below, rather than making this whole type's
/// wire shape ambiguous between two different real consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlRole {
    pub name: String,
    #[serde(default)]
    pub login_actions: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlPlan {
    pub base_url: String,
    pub roles: Vec<CrawlRole>,
}

/// The one, explicit snake_case -> camelCase conversion `materialize_crawl_spec`
/// needs to embed a real plan into a spec `helpers/crawler.ts` can parse —
/// `Action`'s own fields (`url`/`selector`/`value`) need no conversion,
/// only `CrawlPlan`/`CrawlRole`'s own field names do.
fn plan_to_camel_case_json(plan: &CrawlPlan) -> serde_json::Value {
    serde_json::json!({
        "baseUrl": plan.base_url,
        "roles": plan.roles.iter().map(|r| serde_json::json!({
            "name": r.name,
            "loginActions": r.login_actions,
        })).collect::<Vec<_>>(),
    })
}

// --------------------------------------------------------------- validation

/// Structural + SSRF safety validation, same shape
/// `execution_plan::validate_plan` already established (brief §12): reused,
/// not reinvented, for this module's own one real target URL.
pub async fn validate_crawl_plan(plan: &CrawlPlan) -> Result<(), Vec<String>> {
    let mut reasons = Vec::new();

    if let Err(reason) = crate::executor::validate_target_url(&plan.base_url).await {
        reasons.push(reason);
    }

    if plan.roles.is_empty() {
        reasons.push("a crawl plan needs at least one role".to_string());
    }
    let mut seen_names = std::collections::HashSet::new();
    for role in &plan.roles {
        if role.name.trim().is_empty() {
            reasons.push("a role name must not be empty".to_string());
        }
        if !seen_names.insert(role.name.trim().to_lowercase()) {
            reasons.push(format!("duplicate role name: {}", role.name));
        }
        for action in &role.login_actions {
            if !matches!(
                action,
                Action::Navigate { .. } | Action::Fill { .. } | Action::Click { .. }
            ) {
                reasons.push(format!(
                    "role '{}': only NAVIGATE/FILL/CLICK are valid login-sequence actions, got {action:?}",
                    role.name
                ));
            }
        }
    }

    if reasons.is_empty() {
        Ok(())
    } else {
        Err(reasons)
    }
}

// --------------------------------------------------------- materialization

fn safe_id_from(task_id: &str) -> Result<String, String> {
    let safe_id: String = task_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if safe_id.is_empty() {
        return Err(
            "task id produced no safe characters for a generated spec filename".to_string(),
        );
    }
    Ok(safe_id)
}

fn result_file_name(safe_id: &str) -> String {
    format!("crawl-result-{safe_id}.json")
}

fn generated_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../executors/playwright/tests/generated")
}

/// Turns a validated `CrawlPlan` into a real Playwright spec under
/// `executors/playwright/tests/generated/`, executed through the exact
/// same `PlaywrightExecutor`/engine/scheduler pipeline every other browser
/// task already runs through — no new subprocess-management code. Returns
/// the path (relative to `executors/playwright`) `PlaywrightExecutor`'s
/// existing `spec` param already expects.
pub fn materialize_crawl_spec(task_id: &str, plan: &CrawlPlan) -> Result<String, String> {
    let safe_id = safe_id_from(task_id)?;
    let plan_json = serde_json::to_string(&plan_to_camel_case_json(plan))
        .map_err(|e| format!("failed to serialize crawl plan: {e}"))?;
    let file_name = format!("crawl-{safe_id}.spec.ts");
    let dir = generated_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create generated-spec directory: {e}"))?;
    let full_path = dir.join(&file_name);
    let source = format!(
        "// Generated by crawler::materialize_crawl_spec — not hand-edited; safe to delete/regenerate.\n\
         // Contains real, caller-supplied crawl-role credentials in plaintext (see crawler.rs's own\n\
         // module doc comment on this file's at-rest exposure). This directory is gitignored.\n\
         import {{ test }} from \"@playwright/test\";\n\
         import {{ runCrawl, CrawlPlan }} from \"../../helpers/crawler\";\n\n\
         const PLAN: CrawlPlan = {plan_json};\n\n\
         test(\"crawl {safe_id}\", async ({{ browser }}) => {{\n  await runCrawl(browser, PLAN, {result_file});\n}});\n",
        result_file = serde_json::to_string(&result_file_name(&safe_id)).unwrap(),
    );
    std::fs::write(&full_path, source)
        .map_err(|e| format!("failed to write generated crawl spec: {e}"))?;

    Ok(format!("tests/generated/{file_name}"))
}

// ------------------------------------------------------------ result schema

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawledElement {
    pub kind: String,
    pub text: String,
    pub selector_hint: String,
    #[serde(default)]
    pub href: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawledPage {
    pub url: String,
    pub title: String,
    pub elements: Vec<CrawledElement>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleCrawlResult {
    pub role: String,
    pub login_succeeded: bool,
    #[serde(default)]
    pub login_error: Option<String>,
    pub pages: Vec<CrawledPage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisibilityDiffEntry {
    pub url: String,
    pub roles_with_access: Vec<String>,
    pub elements_only_visible_to: std::collections::BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawlOutput {
    pub roles: Vec<RoleCrawlResult>,
    pub visibility_matrix: Vec<VisibilityDiffEntry>,
}

/// Reads back the real, structured result a materialized crawl spec's real
/// run wrote to disk (`helpers/crawler.ts::runCrawl`'s own JSON output) —
/// the one place this module ever *deserializes* `CrawlOutput`; the TS
/// side is the only real producer. `Err` (file missing/unparseable) is a
/// real, honest failure — this never fabricates a placeholder result for a
/// crawl that did not genuinely finish and write one.
pub fn read_crawl_result(task_id: &str) -> Result<CrawlOutput, String> {
    let safe_id = safe_id_from(task_id)?;
    let path = generated_dir().join(result_file_name(&safe_id));
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read crawl result at {}: {e}", path.display()))?;
    serde_json::from_str(&content).map_err(|e| format!("failed to parse crawl result: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(name: &str, login_actions: Vec<Action>) -> CrawlRole {
        CrawlRole {
            name: name.to_string(),
            login_actions,
        }
    }

    // -------------------------------------------------------------- validation

    #[tokio::test]
    async fn validate_crawl_plan_rejects_an_ssrf_target_reusing_the_real_existing_guard() {
        let plan = CrawlPlan {
            base_url: "http://169.254.169.254/".to_string(),
            roles: vec![role("anonymous", vec![])],
        };
        let err = validate_crawl_plan(&plan)
            .await
            .expect_err("cloud metadata address must be rejected");
        assert!(err.iter().any(|r| r.contains("disallowed")), "err={err:?}");
    }

    #[tokio::test]
    async fn validate_crawl_plan_accepts_a_real_well_formed_plan() {
        let plan = CrawlPlan {
            base_url: format!("{}/app", crate::execution_plan::DEFAULT_TARGET),
            roles: vec![
                role(
                    "admin",
                    vec![
                        Action::Navigate {
                            url: format!("{}/login", crate::execution_plan::DEFAULT_TARGET),
                        },
                        Action::Fill {
                            selector: "#username".to_string(),
                            value: "admin".to_string(),
                        },
                        Action::Click {
                            selector: "#submit-login".to_string(),
                        },
                    ],
                ),
                role("anonymous", vec![]),
            ],
        };
        assert!(validate_crawl_plan(&plan).await.is_ok());
    }

    #[tokio::test]
    async fn validate_crawl_plan_rejects_an_empty_role_list() {
        let plan = CrawlPlan {
            base_url: crate::execution_plan::DEFAULT_TARGET.to_string(),
            roles: vec![],
        };
        let err = validate_crawl_plan(&plan).await.expect_err("must reject");
        assert!(err.iter().any(|r| r.contains("at least one role")));
    }

    #[tokio::test]
    async fn validate_crawl_plan_rejects_duplicate_role_names_case_insensitively() {
        let plan = CrawlPlan {
            base_url: crate::execution_plan::DEFAULT_TARGET.to_string(),
            roles: vec![role("Admin", vec![]), role("admin", vec![])],
        };
        let err = validate_crawl_plan(&plan).await.expect_err("must reject");
        assert!(
            err.iter().any(|r| r.contains("duplicate role name")),
            "err={err:?}"
        );
    }

    #[tokio::test]
    async fn validate_crawl_plan_rejects_a_non_browser_action_in_a_login_sequence() {
        let plan = CrawlPlan {
            base_url: crate::execution_plan::DEFAULT_TARGET.to_string(),
            roles: vec![role("admin", vec![Action::AssertStatus { status: 200 }])],
        };
        let err = validate_crawl_plan(&plan).await.expect_err("must reject");
        assert!(
            err.iter().any(|r| r.contains("NAVIGATE/FILL/CLICK")),
            "err={err:?}"
        );
    }

    // ---------------------------------------------------------- materialization

    #[test]
    fn materialize_crawl_spec_embeds_the_real_plan_and_imports_the_real_helper() {
        let plan = CrawlPlan {
            base_url: "http://127.0.0.1:4174".to_string(),
            roles: vec![role(
                "admin",
                vec![Action::Fill {
                    selector: "#username".to_string(),
                    value: "admin".to_string(),
                }],
            )],
        };
        let rel_path =
            materialize_crawl_spec("test-crawl-materialize", &plan).expect("materialize");
        let source = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../executors/playwright")
                .join(&rel_path),
        )
        .expect("read generated spec");
        assert!(source.contains("import { runCrawl, CrawlPlan } from \"../../helpers/crawler\";"));
        assert!(
            source.contains("const PLAN: CrawlPlan ="),
            "the embedded plan must be explicitly typed, or TS widens its literal `type` \
             fields to `string` and the generated spec fails to typecheck: source={source}"
        );
        assert!(source.contains("\"baseUrl\":\"http://127.0.0.1:4174\""));
        assert!(source.contains("\"type\":\"FILL\""));
        assert!(source.contains("\"selector\":\"#username\""));
        assert!(source.contains("crawl-result-test-crawl-materialize.json"));
    }

    #[test]
    fn materialize_crawl_spec_safely_escapes_a_selector_or_value_containing_quotes() {
        let plan = CrawlPlan {
            base_url: "http://127.0.0.1:4174".to_string(),
            roles: vec![role(
                "admin",
                vec![Action::Fill {
                    selector: "#username".to_string(),
                    value: "quote\" and backslash\\ here".to_string(),
                }],
            )],
        };
        let rel_path = materialize_crawl_spec("test-crawl-escaping", &plan).expect("materialize");
        let source = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../executors/playwright")
                .join(&rel_path),
        )
        .expect("read generated spec");
        // A real JSON round trip: re-parsing the embedded PLAN literal (real
        // camelCase, matching what helpers/crawler.ts's own TypeScript side
        // actually parses — see `plan_to_camel_case_json`) must reproduce
        // the exact original value.
        let start =
            source.find("const PLAN: CrawlPlan = ").unwrap() + "const PLAN: CrawlPlan = ".len();
        let end = source[start..].find(";\n").unwrap() + start;
        let parsed: serde_json::Value =
            serde_json::from_str(&source[start..end]).expect("re-parse PLAN");
        match parsed["roles"][0]["loginActions"][0]["value"].as_str() {
            Some(value) => assert_eq!(value, "quote\" and backslash\\ here"),
            other => panic!("expected a real FILL value, got {other:?}"),
        }
    }

    // -------------------------------------------------------------- result parsing

    #[test]
    fn read_crawl_result_parses_the_real_camel_case_json_shape_the_ts_side_produces() {
        let dir = generated_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("crawl-result-test-read-result.json");
        std::fs::write(
            &path,
            r#"{
                "roles": [
                    {"role": "admin", "loginSucceeded": true, "pages": [
                        {"url": "http://x/dashboard", "title": "Dashboard", "elements": [
                            {"kind": "link", "text": "Settings", "selectorHint": "a", "href": "http://x/settings"}
                        ]}
                    ]}
                ],
                "visibilityMatrix": [
                    {"url": "http://x/dashboard", "rolesWithAccess": ["admin"], "elementsOnlyVisibleTo": {"admin": ["Settings"]}}
                ]
            }"#,
        )
        .unwrap();

        let output = read_crawl_result("test-read-result").expect("parse");
        std::fs::remove_file(&path).ok();

        assert_eq!(output.roles.len(), 1);
        assert_eq!(output.roles[0].role, "admin");
        assert!(output.roles[0].login_succeeded);
        assert_eq!(output.roles[0].pages[0].elements[0].text, "Settings");
        assert_eq!(
            output.visibility_matrix[0].elements_only_visible_to["admin"],
            vec!["Settings".to_string()]
        );
    }

    #[test]
    fn read_crawl_result_is_a_real_honest_failure_when_no_result_was_ever_written() {
        let err = read_crawl_result("test-crawl-that-never-ran").expect_err(
            "a missing result file must be a real failure, not a fabricated empty result",
        );
        assert!(err.contains("failed to read crawl result"), "err={err}");
    }
}

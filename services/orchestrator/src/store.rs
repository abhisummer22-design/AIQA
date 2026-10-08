//! Durable persistence (PRD/FRD/SRD "Persistence (Postgres)" gap — the prior
//! session's build report flagged this as the top item under "What Remains":
//! "orchestrator is in-memory only; a restart loses every run."
//!
//! This is backed by SQLite, not the Postgres the target schema
//! (`infra/migrations/`) describes: there is no container runtime in this
//! dev environment (`docker`/`podman` are both absent from PATH — checked
//! before writing this), so `infra/docker-compose.yml`'s Postgres instance
//! can't actually be started here. SQLite gives real, ACID, restart-
//! surviving persistence with zero extra infrastructure. The schema in
//! `migrations/` mirrors `infra/migrations/` column-for-column (see that
//! directory's header comments for the exact deviations), so moving `Store`
//! to talk to Postgres later is a connection-string and type change, not a
//! redesign — every query here goes through named columns, nothing relies on
//! SQLite-only behavior beyond `?` placeholders vs `$n`.
//!
//! One `Store` (an `Arc`-cloneable connection pool wrapper) is created once
//! in `main.rs` and threaded through `AppState`; every module that needs
//! durability (engine.rs, api.rs, auth.rs, policy.rs, intelligence.rs,
//! evidence.rs) goes through it rather than touching SQL directly.

use crate::domain::*;
use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, Sqlite, SqlitePool};
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Read caps
//
// Every table read below this line that used to have no `LIMIT` is append-only
// and grows on its own: `runs` gains a row every run, `execution_telemetry` and
// `healing_proposals` gain rows per failing task, `quality_nodes` per ingested
// task. Their list reads sit behind endpoints the UI polls, and a `.take(n)` in
// the *handler* does not bound the query — by then the whole result set has
// already been materialised (this was already called out for `list_run_ids` at
// `api.rs::list_runs_scoped`). So the bound belongs in the SQL, one named cap
// per table, sized to a page the caller can actually use.
//
// Two of these caps are the *max page the endpoint already clamps to*
// (`list_run_ids` -> `clamp(1, 1000)`), so bounding the query does not change
// what any caller can observe. The rest are deliberately generous: they exist
// to stop a read being unbounded, not to shape a response.
// ---------------------------------------------------------------------------

/// `list_run_ids` -- newest-first run ids. Exactly the ceiling
/// `api.rs::list_runs_scoped` already clamps its `?limit=` to, so the query is
/// bounded without shrinking the largest page that endpoint serves.
const LIST_RUN_IDS_MAX: i64 = 1000;

/// `list_test_cases` -- one project's whole test-case library. Bounded because
/// it is append-only and the list route is polled; `regression_runner` reads
/// `EXECUTED`/`APPROVED` out of it, so the cap is far above any real suite.
const LIST_TEST_CASES_MAX: i64 = 5000;

/// `list_requirements` -- append-only per uploaded document. Large, because
/// these rows drive requirement-to-test-case generation: dropping some here

/// `list_test_intelligence` -- one row per known test node, ordered by risk.
const LIST_TEST_INTELLIGENCE_MAX: i64 = 1000;

/// `list_failure_clusters` -- one row per distinct failure signature, ordered by
/// occurrence count. Distinct signatures, not occurrences, so this is far
/// smaller than `execution_telemetry` in practice.
const LIST_FAILURE_CLUSTERS_MAX: i64 = 1000;

/// `list_healing_proposals` -- append-only, one row per failed task. Note
/// `list_healing_proposals_for_cluster` reads through this, so an investigation
/// correlates against the newest page of proposals, not every proposal the
/// project ever raised. Idempotency itself does NOT go through this read
/// (`has_open_healing_proposal` counts), so the cap cannot cause a duplicate
/// proposal to be raised.
const LIST_HEALING_PROPOSALS_MAX: i64 = 500;

/// `list_quality_nodes` -- one row per ingested task/module, append-only.
const LIST_QUALITY_NODES_MAX: i64 = 5000;

/// `list_quality_relations_for_project` -- the dependency edges between those
/// nodes; grows with the square of them, so it needs a bound more than the
/// nodes do.
const LIST_QUALITY_RELATIONS_MAX: i64 = 20000;

/// `list_test_case_embeddings` -- the brute-force candidate pool

/// One release/build, as stored (Phase 2, QA Intelligence Memory —
/// migrations/0029_qa_releases.sql).
///
/// `label`, `version` and `notes` are all optional **on purpose**: a team may
/// track a label, a version, or neither, and filling a column with an invented
/// value would be fabricating history (master plan §1.4). The UUID is the stable
/// identity; the optional fields are human labels only.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReleaseRow {
    pub id: Uuid,
    pub project_id: String,
    pub label: Option<String>,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub released_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn release_from_row(r: sqlx::sqlite::SqliteRow) -> ReleaseRow {
    ReleaseRow {
        id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
        project_id: r.get("project_id"),
        label: r.get("label"),
        version: r.get("version"),
        notes: r.get("notes"),
        created_by: r.get("created_by"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
        released_at: parse_ts_opt(r.get("released_at")),
    }
}

#[derive(Clone)]
pub struct Store {
    pub pool: SqlitePool,
}

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub(crate) fn parse_ts(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}

pub(crate) fn parse_ts_opt(s: Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
    s.map(|s| parse_ts(&s))
}

fn api_key_row_from(r: sqlx::sqlite::SqliteRow) -> ApiKeyRow {
    ApiKeyRow {
        id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
        name: r.get("name"),
        role: r.get("role"),
        project_id: r.get("project_id"),
    }
}

fn healing_proposal_from_row(r: sqlx::sqlite::SqliteRow) -> Result<HealingProposalRow> {
    Ok(HealingProposalRow {
        id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
        project_id: r.get("project_id"),
        run_id: Uuid::parse_str(&r.get::<String, _>("run_id")).unwrap_or_default(),
        task_id: r.get("task_id"),
        failure_fingerprint: r.get("failure_fingerprint"),
        diagnosis_category: r.get("diagnosis_category"),
        confidence: r.get("confidence"),
        risk_level: r.get("risk_level"),
        requires_approval: r.get::<i64, _>("requires_approval") != 0,
        original_actions: r.get("original_actions"),
        proposed_actions: r.get("proposed_actions"),
        ast_diff: r.get("ast_diff"),
        rationale: r.get("rationale"),
        evidence_refs: r.get("evidence_refs"),
        state: r.get("state"),
        idempotency_key: r.get("idempotency_key"),
        healing_run_id: r
            .get::<Option<String>, _>("healing_run_id")
            .and_then(|s| Uuid::parse_str(&s).ok()),
        verification_result: r.get("verification_result"),
        approved_by: r.get("approved_by"),
        approved_at: parse_ts_opt(r.get("approved_at")),
        rejection_reason: r.get("rejection_reason"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
        expires_at: parse_ts(&r.get::<String, _>("expires_at")),
        updated_at: parse_ts(&r.get::<String, _>("updated_at")),
        context: r.get("context"),
    })
}

fn healing_strategy_metrics_from_row(
    r: sqlx::sqlite::SqliteRow,
) -> Result<HealingStrategyMetricsRow> {
    Ok(HealingStrategyMetricsRow {
        id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
        project_id: r.get("project_id"),
        strategy: r.get("strategy"),
        failure_category: r.get("failure_category"),
        attempts_count: r.get("attempts_count"),
        verified_count: r.get("verified_count"),
        not_verified_count: r.get("not_verified_count"),
        regression_count: r.get("regression_count"),
        calibrated_confidence: r.get("calibrated_confidence"),
        updated_at: parse_ts(&r.get::<String, _>("updated_at")),
    })
}

fn execution_telemetry_from_row(r: sqlx::sqlite::SqliteRow) -> Result<ExecutionTelemetryRow> {
    Ok(ExecutionTelemetryRow {
        id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
        project_id: r.get("project_id"),
        run_id: Uuid::parse_str(&r.get::<String, _>("run_id")).unwrap_or_default(),
        task_id: r.get("task_id"),
        task_name: r.get("task_name"),
        test_case_id: r
            .get::<Option<String>, _>("test_case_id")
            .and_then(|s| Uuid::parse_str(&s).ok()),
        execution_id: r.get("execution_id"),
        executor: r.get("executor"),
        environment: r.get("environment"),
        start_time: parse_ts(&r.get::<String, _>("start_time")),
        end_time: parse_ts_opt(r.get("end_time")),
        duration_ms: r.get("duration_ms"),
        status: r.get("status"),
        retry_count: r.get("retry_count"),
        healing_attempt_count: r.get("healing_attempt_count"),
        failure_category: r.get("failure_category"),
        failure_fingerprint: r.get("failure_fingerprint"),
        error_message: r.get("error_message"),
        evidence_refs: r.get("evidence_refs"),
        assertion_result: r.get("assertion_result"),
        http_status: r.get("http_status"),
        selector: r.get("selector"),
        verification_outcome: r.get("verification_outcome"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
    })
}

fn performance_run_from_row(r: sqlx::sqlite::SqliteRow) -> Result<PerformanceRunRow> {
    Ok(PerformanceRunRow {
        id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
        project_id: r.get("project_id"),
        name: r.get("name"),
        test_type: r.get("test_type"),
        status: r.get("status"),
        verdict: r.get("verdict"),
        config_json: r.get("config_json"),
        metrics_json: r.get("metrics_json"),
        verdict_reasons_json: r.get("verdict_reasons_json"),
        error_reason: r.get("error_reason"),
        evidence_id: r
            .get::<Option<String>, _>("evidence_id")
            .and_then(|s| Uuid::parse_str(&s).ok()),
        created_by: r.get("created_by"),
        started_at: parse_ts(&r.get::<String, _>("started_at")),
        finished_at: parse_ts_opt(r.get("finished_at")),
    })
}

fn test_case_from_row(r: sqlx::sqlite::SqliteRow) -> Result<TestCaseRow> {
    Ok(TestCaseRow {
        id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
        project_id: r.get("project_id"),
        title: r.get("title"),
        description: r.get("description"),
        category: r.get("category"),
        priority: r.get("priority"),
        severity: r.get("severity"),
        preconditions: serde_json::from_str(&r.get::<String, _>("preconditions"))?,
        steps: serde_json::from_str(&r.get::<String, _>("steps"))?,
        expected_result: r.get("expected_result"),
        assertions: serde_json::from_str(&r.get::<String, _>("assertions"))?,
        test_data: r.get("test_data"),
        source_document: r.get("source_document"),
        requirement_reference: r.get("requirement_reference"),
        source_requirement: r.get("source_requirement"),
        traceability: r
            .get::<Option<String>, _>("traceability")
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        risk_factor: r.get("risk_factor"),
        tags: serde_json::from_str(&r.get::<String, _>("tags"))?,
        source: r.get("source"),
        status: TestCaseStatus::from_db_str(&r.get::<String, _>("status")),
        validation_notes: r.get("validation_notes"),
        executed_run_id: r
            .get::<Option<String>, _>("executed_run_id")
            .and_then(|s| Uuid::parse_str(&s).ok()),
        executed_task_id: r.get("executed_task_id"),
        created_by: r.get("created_by"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
        updated_at: parse_ts(&r.get::<String, _>("updated_at")),
        suite_id: r
            .get::<Option<String>, _>("suite_id")
            .and_then(|s| Uuid::parse_str(&s).ok()),
    })
}

/// Vector Store v1. `test_case_id` here is trusted to be well-formed (this
/// process is the only writer, via `upsert_test_case_embedding`'s own
/// `Uuid::to_string()`) — a real parse failure is a genuine data-integrity
/// problem worth surfacing as an error, unlike `test_case_from_row`'s own
/// looser `.unwrap_or_default()` on `id` above.
fn test_case_embedding_from_row(r: sqlx::sqlite::SqliteRow) -> Result<TestCaseEmbeddingRow> {
    Ok(TestCaseEmbeddingRow {
        test_case_id: Uuid::parse_str(&r.get::<String, _>("test_case_id"))?,
        embedding: serde_json::from_str(&r.get::<String, _>("embedding"))?,
    })
}

/// Phase 2 (document-level RAG). `id` here is trusted to be well-formed —
/// this process is the only writer, via `upsert_document`'s own
/// `Uuid::to_string()` — same reasoning `test_case_embedding_from_row`
/// above already documents for its own id.
fn document_embedding_from_row(r: sqlx::sqlite::SqliteRow) -> Result<DocumentEmbeddingRow> {
    Ok(DocumentEmbeddingRow {
        id: Uuid::parse_str(&r.get::<String, _>("id"))?,
        embedding: serde_json::from_str(&r.get::<String, _>("embedding"))?,
    })
}

fn document_from_row(r: sqlx::sqlite::SqliteRow) -> Result<DocumentRow> {
    Ok(DocumentRow {
        id: Uuid::parse_str(&r.get::<String, _>("id"))?,
        project_id: r.get("project_id"),
        file_name: r.get("file_name"),
        text: r.get("text"),
        char_count: r.get::<i64, _>("char_count"),
        truncated: r.get::<i64, _>("truncated") != 0,
    })
}

fn requirement_document_from_row(r: sqlx::sqlite::SqliteRow) -> Result<RequirementDocumentRow> {
    Ok(RequirementDocumentRow {
        id: Uuid::parse_str(&r.get::<String, _>("id"))?,
        file_name: r.get("file_name"),
        doc_type: r.get("doc_type"),
        char_count: r.get("char_count"),
        requirements_count: r.get("requirements_count"),
        sections_count: r.get("sections_count"),
        skipped_sections: serde_json::from_str(&r.get::<String, _>("skipped_sections"))
            .unwrap_or_default(),
        furniture_lines: r.get("furniture_lines"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
        updated_at: parse_ts(&r.get::<String, _>("updated_at")),
    })
}

fn requirement_from_row(r: sqlx::sqlite::SqliteRow) -> Result<RequirementRow> {
    Ok(RequirementRow {
        document_id: Uuid::parse_str(&r.get::<String, _>("document_id"))?,
        key: r.get("req_key"),
        ordinal: r.get("ordinal"),
        section: r.get("section"),
        kind: r.get("kind"),
        text: r.get("text"),
    })
}

fn test_plan_from_row(r: sqlx::sqlite::SqliteRow) -> Result<TestPlanRow> {
    Ok(TestPlanRow {
        id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
        project_id: r.get("project_id"),
        name: r.get("name"),
        description: r.get("description"),
        status: TestPlanStatus::from_db_str(&r.get::<String, _>("status")),
        entry_criteria: r.get("entry_criteria"),
        exit_criteria: r.get("exit_criteria"),
        created_by: r.get("created_by"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
        updated_at: parse_ts(&r.get::<String, _>("updated_at")),
        suite_count: r.get("suite_count"),
    })
}

fn test_suite_from_row(r: sqlx::sqlite::SqliteRow) -> Result<TestSuiteRow> {
    Ok(TestSuiteRow {
        id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
        project_id: r.get("project_id"),
        plan_id: r
            .get::<Option<String>, _>("plan_id")
            .and_then(|s| Uuid::parse_str(&s).ok()),
        name: r.get("name"),
        description: r.get("description"),
        created_by: r.get("created_by"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
        updated_at: parse_ts(&r.get::<String, _>("updated_at")),
        case_count: r.get("case_count"),
    })
}

fn project_from_row(r: sqlx::sqlite::SqliteRow) -> Project {
    Project {
        id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
        name: r.get("name"),
        description: r.get("description"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
        updated_at: parse_ts(&r.get::<String, _>("updated_at")),
    }
}

fn custom_skill_from_row(r: sqlx::sqlite::SqliteRow) -> CustomSkillRow {
    CustomSkillRow {
        id: r.get("id"),
        name: r.get("name"),
        description: r.get("description"),
        source_file: r.get("source_file"),
        // Real frontmatter from an imported SKILL.md, as the file wrote it.
        // Unparseable or absent stays `None` rather than becoming `{}` — a
        // hand-made skill genuinely has no frontmatter, and that is a
        // different fact from "it had empty frontmatter".
        metadata: r
            .get::<Option<String>, _>("metadata")
            .and_then(|m| serde_json::from_str(&m).ok()),
        created_by: r.get("created_by"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
    }
}

fn custom_ai_provider_from_row(r: sqlx::sqlite::SqliteRow) -> CustomAiProviderRow {
    CustomAiProviderRow {
        id: r.get("id"),
        name: r.get("name"),
        base_url: r.get("base_url"),
        api_key: r.get("api_key"),
        input_usd_per_1k: r.get("input_usd_per_1k"),
        output_usd_per_1k: r.get("output_usd_per_1k"),
        created_by: r.get("created_by"),
        created_at: parse_ts(&r.get::<String, _>("created_at")),
    }
}

impl Store {
    pub async fn connect(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // B1 — the orchestrator writes concurrently from many task futures.
        // Under the default rollback journal every write takes an exclusive
        // lock on the whole database, and with no busy timeout a blocked
        // writer fails *immediately* with SQLITE_BUSY. That is the root
        // cause of the silent durable-state corruption in the backend audit
        // (a completed run persisted as FAILED, passed tasks left RUNNING).
        //
        //   journal_mode = WAL  -> readers never block writers, and writers
        //                          serialise without locking out readers.
        //   busy_timeout = 5s   -> a contended writer waits and retries
        //                          instead of erroring out on first contact.
        //   synchronous = NORMAL-> the safe/standard pairing with WAL; FULL
        //                          is only needed to survive OS-level crash,
        //                          not process crash, and costs an fsync per
        //                          commit.
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    /// A transaction that holds SQLite's write lock from its first statement (`BEGIN IMMEDIATE`). Use it
    /// for every transaction that reads before it writes.
    ///
    /// A plain `self.pool.begin()` is deferred: it takes a read snapshot at its first `SELECT` and asks
    /// for the write lock only at its first write. If another connection holds the lock or has committed
    /// since that read (the background writers still finishing after a run do), SQLite refuses the write
    /// at once with `database is locked` (SQLITE_BUSY, or SQLITE_BUSY_SNAPSHOT, code 517): it never runs
    /// the busy handler for a read that is being upgraded to a write, so the `busy_timeout` set in
    /// `connect` does not apply. Taking the lock at `BEGIN`, before anything is read, waits for it under
    /// that timeout instead. A transaction whose first statement is a write needs none of this.
    pub(crate) async fn begin_write(&self) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>> {
        Ok(self.pool.begin_with("BEGIN IMMEDIATE").await?)
    }

    // ---------------------------------------------------------------- runs

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_run(
        &self,
        run_id: RunId,
        plan: &RunPlan,
        project_id: Option<&str>,
        requested_by: Option<&str>,
        state: RunState,
        policy_notes: Option<&str>,
    ) -> Result<()> {
        self.insert_run_for_release(
            run_id,
            plan,
            project_id,
            requested_by,
            state,
            policy_notes,
            None,
        )
        .await
    }

    /// Phase 2: `insert_run` plus an optional release association.
    ///
    /// `release_id` must name a release **of this run's own project**. A release
    /// from another project is refused rather than attached, so a run can never
    /// enter a tenant's release history by pointing at another tenant's release.
    /// `None` is the ordinary case and stays `NULL` — nothing is auto-attached, so
    /// a release-less run is honestly absent from every release view.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_run_for_release(
        &self,
        run_id: RunId,
        plan: &RunPlan,
        project_id: Option<&str>,
        requested_by: Option<&str>,
        state: RunState,
        policy_notes: Option<&str>,
        release_id: Option<&str>,
    ) -> Result<()> {
        if let (Some(rel), Some(pid)) = (release_id, project_id) {
            let owned = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM qa_releases WHERE id = ? AND project_id = ?",
            )
            .bind(rel)
            .bind(pid)
            .fetch_one(&self.pool)
            .await?;
            if owned == 0 {
                return Err(anyhow::anyhow!(
                    "release {rel} does not belong to project {pid}"
                ));
            }
        }
        // B6 — this used to issue one un-batched INSERT per task and per
        // dependency, each in its own implicit transaction (and therefore
        // its own fsync). A 100-task plan took ~14s and then failed; 500
        // tasks failed outright. Two changes fix it:
        //
        //   1. ONE transaction for the whole plan. A partially-inserted run
        //      is now impossible — previously a failure mid-loop left a run
        //      row with an incomplete task set and no rollback.
        //   2. Multi-row INSERTs in bounded chunks, so 1,000 tasks become a
        //      handful of statements rather than 1,000 round trips.
        //
        // CHUNK is deliberately conservative: SQLite's default bound-variable
        // ceiling (SQLITE_MAX_VARIABLE_NUMBER) is 999 on older builds, and
        // tasks bind 4 variables per row, so 200 rows = 800 variables stays
        // safely inside it on every build.
        const CHUNK: usize = 200;

        let plan_json = serde_json::to_string(plan)?;
        let run_id_s = run_id.to_string();

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO runs (id, name, project_id, state, concurrency, plan, created_at, requested_by, policy_notes, release_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&run_id_s)
        .bind(&plan.name)
        .bind(project_id)
        .bind(state.as_db_str())
        .bind(plan.concurrency as i64)
        .bind(plan_json)
        .bind(now_rfc3339())
        .bind(requested_by)
        .bind(policy_notes)
        // Phase 2: NULL unless a caller explicitly named this run's release.
        .bind(release_id)
        .execute(&mut *tx)
        .await?;

        // `QueryBuilder` rather than a `format!`ed string: sqlx 0.9 refuses
        // runtime-built SQL at compile time (injection guard), and this is
        // the API it provides for exactly this multi-row-insert case.
        for chunk in plan.tasks.chunks(CHUNK) {
            let mut qb = sqlx::QueryBuilder::new(
                "INSERT INTO tasks (id, run_id, name, status, executor_kind, test_case_id) ",
            );
            qb.push_values(chunk, |mut b, t| {
                b.push_bind(t.id.clone())
                    .push_bind(run_id_s.clone())
                    .push_bind(t.name.clone())
                    .push_bind("QUEUED")
                    .push_bind(t.executor.clone())
                    // Phase 2: NULL for every task that is not one test case's
                    // execution (a crawl, a hand-built API task, a direct
                    // RunPlan). Never inferred from the task name.
                    .push_bind(t.test_case_id.clone());
            });
            qb.build().execute(&mut *tx).await?;
        }

        // Dependency edges, flattened first so they chunk evenly regardless
        // of how unevenly prerequisites are distributed across tasks.
        let edges: Vec<(&str, &str)> = plan
            .tasks
            .iter()
            .flat_map(|t| {
                t.prerequisites
                    .iter()
                    .map(move |p| (t.id.as_str(), p.as_str()))
            })
            .collect();

        for chunk in edges.chunks(CHUNK) {
            let mut qb = sqlx::QueryBuilder::new(
                "INSERT INTO task_dependencies (run_id, task_id, prerequisite_id) ",
            );
            qb.push_values(chunk, |mut b, (task_id, prereq)| {
                b.push_bind(run_id_s.clone())
                    .push_bind(task_id.to_string())
                    .push_bind(prereq.to_string());
            });
            qb.build().execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn set_run_state(&self, run_id: RunId, state: RunState) -> Result<()> {
        sqlx::query("UPDATE runs SET state = ? WHERE id = ?")
            .bind(state.as_db_str())
            .bind(run_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_run_started(&self, run_id: RunId) -> Result<()> {
        sqlx::query("UPDATE runs SET started_at = ? WHERE id = ? AND started_at IS NULL")
            .bind(now_rfc3339())
            .bind(run_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_run_finished(
        &self,
        run_id: RunId,
        state: RunState,
        policy_notes: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE runs SET state = ?, finished_at = ?, policy_notes = COALESCE(?, policy_notes) WHERE id = ?",
        )
        .bind(state.as_db_str())
        .bind(now_rfc3339())
        .bind(policy_notes)
        .bind(run_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn approve_run(&self, run_id: RunId, approved_by: &str) -> Result<()> {
        sqlx::query(
            "UPDATE runs SET state = 'SCHEDULED', approved_by = ?, approved_at = ? WHERE id = ?",
        )
        .bind(approved_by)
        .bind(now_rfc3339())
        .bind(run_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn reject_run(&self, run_id: RunId, rejected_by: &str, reason: &str) -> Result<()> {
        sqlx::query(
            "UPDATE runs SET state = 'REJECTED', approved_by = ?, approved_at = ?, finished_at = ?, policy_notes = ? WHERE id = ?",
        )
        .bind(rejected_by)
        .bind(now_rfc3339())
        .bind(now_rfc3339())
        .bind(reason)
        .bind(run_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_task_status(
        &self,
        run_id: RunId,
        task_id: &str,
        status: TaskStatus,
        reason: Option<&str>,
        set_started: bool,
        set_finished: bool,
    ) -> Result<()> {
        let now = now_rfc3339();
        if set_started {
            sqlx::query("UPDATE tasks SET status = ?, started_at = ? WHERE run_id = ? AND id = ?")
                .bind(status.as_db_str())
                .bind(&now)
                .bind(run_id.to_string())
                .bind(task_id)
                .execute(&self.pool)
                .await?;
        } else if set_finished {
            sqlx::query(
                "UPDATE tasks SET status = ?, reason = ?, finished_at = ? WHERE run_id = ? AND id = ?",
            )
            .bind(status.as_db_str())
            .bind(reason)
            .bind(&now)
            .bind(run_id.to_string())
            .bind(task_id)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query("UPDATE tasks SET status = ?, reason = ? WHERE run_id = ? AND id = ?")
                .bind(status.as_db_str())
                .bind(reason)
                .bind(run_id.to_string())
                .bind(task_id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn insert_event(&self, ev: &RunEvent) -> Result<()> {
        sqlx::query(
            "INSERT INTO run_events (event_id, run_id, task_id, sequence, event_type, status, summary, schema_version, occurred_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(ev.event_id.to_string())
        .bind(ev.run_id.to_string())
        .bind(&ev.task_id)
        .bind(ev.sequence as i64)
        .bind(ev.event_type.as_db_str())
        .bind(ev.status.map(|s| s.as_db_str()))
        .bind(&ev.summary)
        .bind(ev.schema_version as i64)
        .bind(ev.timestamp.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_events_since(&self, run_id: RunId, since: u64) -> Result<Vec<RunEvent>> {
        let rows = sqlx::query(
            "SELECT event_id, run_id, task_id, sequence, event_type, status, summary, schema_version, occurred_at
             FROM run_events WHERE run_id = ? AND sequence >= ? ORDER BY sequence ASC",
        )
        .bind(run_id.to_string())
        .bind(since as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| RunEvent {
                event_id: Uuid::parse_str(&r.get::<String, _>("event_id")).unwrap_or_default(),
                run_id,
                task_id: r.get("task_id"),
                sequence: r.get::<i64, _>("sequence") as u64,
                timestamp: parse_ts(&r.get::<String, _>("occurred_at")),
                schema_version: r.get::<i64, _>("schema_version") as u32,
                event_type: EventType::from_db_str(&r.get::<String, _>("event_type")),
                status: r
                    .get::<Option<String>, _>("status")
                    .map(|s| TaskStatus::from_db_str(&s)),
                summary: r.get("summary"),
                // BE-14 — `run_events` has no `project_id` column (see
                // `RunEvent`'s doc comment); honestly `None` rather than
                // guessed for an event reconstructed from the DB.
                project_id: None,
                project_sequence: None,
            })
            .collect())
    }

    /// A run as persisted, independent of whether it's still in the
    /// in-memory `AppState.runs` map (SRD §18 reconnect path extended to
    /// survive a full process restart, not just a WebSocket drop).
    pub async fn get_run_summary(&self, run_id: RunId) -> Result<Option<RunSummary>> {
        let row = sqlx::query(
            "SELECT id, name, project_id, state, created_at, started_at, finished_at, requested_by, policy_notes, approved_by, approved_at
             FROM runs WHERE id = ?",
        )
        .bind(run_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };

        let tasks = self.list_tasks_for_run(run_id).await?;
        let mut counts = std::collections::HashMap::new();
        for t in &tasks {
            *counts.entry(t.status).or_insert(0usize) += 1;
        }
        let get = |s: TaskStatus| counts.get(&s).copied().unwrap_or(0);

        Ok(Some(RunSummary {
            run_id,
            name: row.get("name"),
            state: RunState::from_db_str(&row.get::<String, _>("state")),
            total: tasks.len(),
            queued: get(TaskStatus::Queued),
            running: get(TaskStatus::Running),
            passed: get(TaskStatus::Passed),
            failed: get(TaskStatus::Failed),
            blocked: get(TaskStatus::Blocked),
            skipped: get(TaskStatus::Skipped),
            cancelled: get(TaskStatus::Cancelled),
            timeout: get(TaskStatus::Timeout),
            infra_error: get(TaskStatus::InfraError),
            needs_review: get(TaskStatus::NeedsReview),
            started_at: parse_ts_opt(row.get("started_at")).unwrap_or_else(chrono::Utc::now),
            finished_at: parse_ts_opt(row.get("finished_at")),
            project_id: row.get("project_id"),
            requested_by: row.get("requested_by"),
            policy_notes: row.get("policy_notes"),
            approved_by: row.get("approved_by"),
            approved_at: parse_ts_opt(row.get("approved_at")),
        }))
    }

    /// B11/L4 remediation. Everything needed to rebuild a live
    /// `Arc<RunRecord>` (`engine::RunRecord::new`) for a run that is not
    /// held in this process's memory — the one case that actually needs
    /// it is a `PENDING_APPROVAL` run whose creating process has since
    /// restarted (see `api.rs::rehydrate_run_record`). This is possible at
    /// all only because `insert_run` already persists the *entire* plan
    /// verbatim as JSON in the `plan` column, not just a summary of it —
    /// reconstruction is a deserialize, not a re-derivation from
    /// normalized task/dependency rows. Returns `None` if the run doesn't
    /// exist; returns `Some` for a run in *any* state (the caller — not
    /// this function — decides whether the current state is actually
    /// valid to act on, exactly like `get_run_summary` above does for
    /// reads).
    pub async fn get_run_for_rehydration(
        &self,
        run_id: RunId,
    ) -> Result<
        Option<(
            RunPlan,
            Option<String>,
            Option<String>,
            Option<String>,
            RunState,
        )>,
    > {
        let row = sqlx::query(
            "SELECT plan, project_id, requested_by, policy_notes, state FROM runs WHERE id = ?",
        )
        .bind(run_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let plan_json: String = row.get("plan");
        let plan: RunPlan = serde_json::from_str(&plan_json)?;
        Ok(Some((
            plan,
            row.get("project_id"),
            row.get("requested_by"),
            row.get("policy_notes"),
            RunState::from_db_str(&row.get::<String, _>("state")),
        )))
    }

    pub async fn list_tasks_for_run(&self, run_id: RunId) -> Result<Vec<TaskState>> {
        let rows = sqlx::query(
            "SELECT id, name, status, reason, started_at, finished_at, test_case_id FROM tasks WHERE run_id = ? ORDER BY id",
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let deps =
            sqlx::query("SELECT task_id, prerequisite_id FROM task_dependencies WHERE run_id = ?")
                .bind(run_id.to_string())
                .fetch_all(&self.pool)
                .await?;
        let mut dep_map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for d in deps {
            dep_map
                .entry(d.get("task_id"))
                .or_default()
                .push(d.get("prerequisite_id"));
        }
        Ok(rows
            .into_iter()
            .map(|r| {
                let id: String = r.get("id");
                TaskState {
                    prerequisites: dep_map.remove(&id).unwrap_or_default(),
                    id,
                    name: r.get("name"),
                    status: TaskStatus::from_db_str(&r.get::<String, _>("status")),
                    reason: r.get("reason"),
                    started_at: parse_ts_opt(r.get("started_at")),
                    finished_at: parse_ts_opt(r.get("finished_at")),
                    // Phase 2: the durable case<->execution link, read back from
                    // the column migration 0028 added.
                    test_case_id: r.get("test_case_id"),
                }
            })
            .collect())
    }

    // ============================ Phase 2: QA Intelligence Memory foundation
    //
    // Everything below exists to make historical QA intelligence retrievable, and
    // every one of it reuses `execution_telemetry` rather than standing up a
    // second memory store (see docs/qa-intelligence-memory/PHASE_1_AUDIT.md §7).

    /// Records a release for a project.
    ///
    /// A duplicate `label` inside the same project is refused by the UNIQUE index
    /// (migration 0029). Across projects the same label is a different release,
    /// which is what makes labels usable as a human key without making them an
    /// identity.
    pub async fn create_release(
        &self,
        project_id: &str,
        label: Option<&str>,
        version: Option<&str>,
        notes: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<ReleaseRow> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO qa_releases (id, project_id, label, version, notes, created_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(project_id)
        .bind(label)
        .bind(version)
        .bind(notes)
        .bind(created_by)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;

        let inserted = sqlx::query("SELECT * FROM qa_releases WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&self.pool)
            .await?;
        Ok(release_from_row(inserted))
    }

    /// Releases of one project, newest first. Strictly project-scoped: this is
    /// the only read path Phase 5 needs and it never sees another project's rows.
    pub async fn list_releases(&self, project_id: &str) -> Result<Vec<ReleaseRow>> {
        let rows = sqlx::query(
            "SELECT * FROM qa_releases WHERE project_id = ? ORDER BY created_at DESC, id DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(release_from_row).collect())
    }

    /// Every run recorded against one release, newest first.
    ///
    /// Runs with no release are absent from every release view - never attached to
    /// one by default - so release intelligence can answer INSUFFICIENT_DATA
    /// instead of inventing an association.
    pub async fn runs_for_release(&self, release_id: Uuid) -> Result<Vec<Uuid>> {
        let rows = sqlx::query("SELECT id FROM runs WHERE release_id = ? ORDER BY created_at DESC")
            .bind(release_id.to_string())
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| Uuid::parse_str(&r.get::<String, _>("id")).ok())
            .collect())
    }

    /// Records a module as a `quality_nodes` vertex of type `module` and links a
    /// test case to it.
    ///
    /// Phase 2 (QA Intelligence Memory): a module is modelled on the **existing**
    /// quality graph rather than in a table of its own. `quality_nodes` is already
    /// a generic typed graph used for `test`, `failure_cluster` and requirement
    /// vertices, with identity `(project_id, node_type, external_ref)`
    /// (`upsert_quality_node`), and `quality_relations` already carries typed
    /// edges (`validates`, `fails_with`). A module is exactly the same kind of
    /// vertex, so a `module` table would have been a fourth parallel identity
    /// system for no gain (master plan §1.3).
    ///
    /// The edge is named `covers`, case -> module, to sit alongside the existing
    /// direction of `validates` (case -> requirement) and `fails_with`
    /// (case -> cluster): all three point from the test case outwards.
    ///
    /// Returns the module node id. A blank `module_ref` is refused rather than
    /// creating an unnamed vertex.
    pub async fn upsert_module_node(
        &self,
        project_id: &str,
        module_ref: &str,
        name: &str,
    ) -> Result<Uuid> {
        let module_ref = module_ref.trim();
        if module_ref.is_empty() {
            return Err(anyhow::anyhow!("a module reference cannot be empty"));
        }
        let module_node_id = self
            .upsert_quality_node(project_id, "module", module_ref, name)
            .await?;
        Ok(module_node_id)
    }

    /// Links one test case's `test` vertex to a module's `module` vertex.
    ///
    /// Both vertices are created if absent, so a caller never has to order its
    /// writes. Idempotent: `upsert_quality_relation` is a read-then-insert against
    /// the `(from, to, type)` unique key, so repeating it is a no-op.
    pub async fn link_test_case_to_module(
        &self,
        project_id: &str,
        case_ref: &str,
        module_ref: &str,
        case_name: &str,
        module_name: &str,
    ) -> Result<()> {
        let module_node_id = self
            .upsert_module_node(project_id, module_ref, module_name)
            .await?;
        let case_node_id = self
            .upsert_quality_node(project_id, "test", case_ref, case_name)
            .await?;
        self.upsert_quality_relation(case_node_id, module_node_id, "covers")
            .await?;
        Ok(())
    }

    /// Recovers `tasks.test_case_id` for rows written before migration 0028, by
    /// reading the case id out of the `runs.plan` JSON that has contained it all
    /// along.
    ///
    /// Returns how many rows it filled.
    ///
    /// Two rules, both from master plan §1.4 ("historical claims must be
    /// evidence-based") and §8.5:
    ///
    ///  * **Only unambiguous runs are touched.** A run is resolved only when every
    ///    task in its plan carries exactly one `test_case_id`. A plan naming zero
    ///    or several distinct cases leaves its tasks NULL, because which task
    ///    belongs to which case is not recorded anywhere and guessing would invent
    ///    history.
    ///  * **Only this project.** `project_id` is required and every statement is
    ///    scoped by it, so one tenant's history can never be written under
    ///    another's.
    ///
    /// Idempotent by construction: a row already carrying a value is skipped.
    pub async fn backfill_task_test_case_links(&self, project_id: &str) -> Result<u64> {
        let runs = sqlx::query("SELECT id, plan FROM runs WHERE project_id = ?")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;

        let mut filled = 0u64;
        for run in runs {
            let run_id: String = run.get("id");
            let plan_json: String = run.get("plan");

            // Resolve the plan's tasks -> case ids. Anything but a clean
            // one-case-per-task mapping is left alone.
            let pairs: Vec<(String, String)> =
                match serde_json::from_str::<serde_json::Value>(&plan_json) {
                    Ok(serde_json::Value::Object(map)) => {
                        let mut out: Vec<(String, String)> = Vec::new();
                        for task in map
                            .get("tasks")
                            .and_then(|t| t.as_array())
                            .cloned()
                            .unwrap_or_default()
                        {
                            let (Some(id), Some(case)) = (
                                task.get("id").and_then(|v| v.as_str()),
                                task.get("test_case_id").and_then(|v| v.as_str()),
                            ) else {
                                // A task with no case id makes the whole run
                                // partially unattributable; skip the run rather than
                                // guess at the remaining tasks.
                                out.clear();
                                break;
                            };
                            // A malformed id is not a link; skip the run.
                            if uuid::Uuid::parse_str(case).is_err() {
                                out.clear();
                                break;
                            }
                            out.push((id.to_string(), case.to_string()));
                        }
                        out
                    }
                    // An unreadable plan is not evidence of anything.
                    _ => Vec::new(),
                };
            if pairs.is_empty() {
                continue;
            }

            for (task_id, case_id) in pairs {
                let res = sqlx::query(
                    "UPDATE tasks SET test_case_id = ?
                     WHERE id = ? AND run_id = ? AND test_case_id IS NULL",
                )
                .bind(&case_id)
                .bind(&task_id)
                .bind(&run_id)
                .execute(&self.pool)
                .await?;
                filled += res.rows_affected();
            }
        }
        Ok(filled)
    }

    /// The `CREATE TABLE` statement SQLite holds for one table, for the migration
    /// tests that assert the schema rather than trusting a comment.
    ///
    /// Read from `sqlite_master` with a **bound** parameter: `pragma_table_info`
    /// needs the table name as a literal, and sqlx refuses runtime-built SQL, so the
    /// DDL text is both the statically-valid way to ask and the stronger assertion —
    /// it checks the statement the migration actually ran, not a re-derivation of it.
    ///
    /// `None` when no such table exists.
    pub async fn table_ddl(&self, table: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("sql")))
    }

    /// Whether a table exists. Bound, so the name is never interpolated.
    pub async fn table_exists(&self, table: &str) -> Result<bool> {
        let n = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(table)
        .fetch_one(&self.pool)
        .await?;
        Ok(n > 0)
    }

    /// Every run id known to the DB, most recent first — the "history" list
    /// that survives a restart, unlike the old in-memory-only `list_runs`.
    /// `project_filter`: `None` sees every project (admin/global keys);
    /// `Some(pid)` restricts to that project (SRD/PRD multi-tenancy gap).
    ///
    /// Bounded by `LIST_RUN_IDS_MAX`, which is the same ceiling
    /// `api.rs::list_runs_scoped` clamps its `?limit=` to. The `ORDER BY
    /// created_at DESC` is what makes the handler's `.take(limit)` and this cap
    /// agree: both keep the newest ids, so a caller asking for the first 50 is
    /// served the same 50 rows it always was.
    pub async fn list_run_ids(&self, project_filter: Option<&str>) -> Result<Vec<Uuid>> {
        let rows = if let Some(pid) = project_filter {
            sqlx::query("SELECT id FROM runs WHERE project_id = ? ORDER BY created_at DESC LIMIT ?")
                .bind(pid)
                .bind(LIST_RUN_IDS_MAX)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query("SELECT id FROM runs ORDER BY created_at DESC LIMIT ?")
                .bind(LIST_RUN_IDS_MAX)
                .fetch_all(&self.pool)
                .await?
        };
        Ok(rows
            .into_iter()
            .filter_map(|r| Uuid::parse_str(&r.get::<String, _>("id")).ok())
            .collect())
    }

    // ------------------------------------------------------ dashboard / kpis

    /// BE-12 (API Contract §23.1). Run counts by state for one project — the
    /// same `GROUP BY state` shape `metrics.rs::render` already uses for the
    /// global Prometheus counter, scoped down to a single project.
    pub async fn dashboard_run_counts(&self, project_id: &str) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT state, COUNT(*) as c FROM runs WHERE project_id = ? GROUP BY state",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("state"), r.get("c")))
            .collect())
    }

    /// Task counts by status for one project. `tasks` carries no
    /// `project_id` of its own, so the tenant is resolved via the owning
    /// run — the same join `task_history_by_name` already uses.
    pub async fn dashboard_task_counts(&self, project_id: &str) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT t.status as status, COUNT(*) as c
             FROM tasks t JOIN runs r ON r.id = t.run_id
             WHERE r.project_id = ?
             GROUP BY t.status",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("status"), r.get("c")))
            .collect())
    }

    /// Real size of this project's quality graph — the knowledge structure
    /// the Neural Pipeline view reports as its "memory". Both numbers are
    /// plain counts of rows that genuinely exist (`quality_nodes` and the
    /// `quality_relations` edges between them, migration
    /// `0002_quality_graph_and_intelligence.sql`); nothing is derived or
    /// estimated. Zero is a real, honest answer for a project that has
    /// never executed anything.
    pub async fn quality_graph_size(&self, project_id: &str) -> Result<(i64, i64)> {
        let nodes = sqlx::query("SELECT COUNT(*) as c FROM quality_nodes WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(&self.pool)
            .await?
            .get::<i64, _>("c");
        let relations = sqlx::query(
            "SELECT COUNT(*) as c FROM quality_relations WHERE from_node_id IN (SELECT id FROM quality_nodes WHERE project_id = ?)",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?
        .get::<i64, _>("c");
        Ok((nodes, relations))
    }

    /// Distinct known tests for one project: `quality_nodes` rows of type
    /// `'test'` (see `intelligence.rs::update_test_intelligence`), which is
    /// this backend's only real "test" identity — there is no separate
    /// generated-test-case table (Test Case Generator, §8/§9, is not built).
    pub async fn dashboard_test_count(&self, project_id: &str) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(*) as c FROM quality_nodes WHERE project_id = ? AND node_type = 'test'",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("c"))
    }

    /// Average task duration in milliseconds for one project, over tasks
    /// that have both `started_at` and `finished_at` recorded. `None` when
    /// no such task exists yet — an honest "no data", never a fabricated 0
    /// (BE-12 data-integrity rule / FE-10's own "unavailable -> `—`, never
    /// `0`" convention).
    pub async fn dashboard_avg_task_duration_ms(&self, project_id: &str) -> Result<Option<f64>> {
        let row = sqlx::query(
            "SELECT AVG((julianday(t.finished_at) - julianday(t.started_at)) * 86400000.0) as avg_ms
             FROM tasks t JOIN runs r ON r.id = t.run_id
             WHERE r.project_id = ? AND t.started_at IS NOT NULL AND t.finished_at IS NOT NULL",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("avg_ms"))
    }

    /// BE-12b — average *run* duration in milliseconds for one project
    /// (distinct from `dashboard_avg_task_duration_ms` above, which is per
    /// task). Same real columns (`runs.started_at`/`.finished_at`, present
    /// since migration `0001_core_execution.sql`), same shape of query, one
    /// join simpler (no `tasks` join needed — both timestamps already live
    /// on `runs` itself). Deliberately no `state` filter, matching the
    /// task-duration query's own methodology: every terminal run with both
    /// timestamps counts, whatever its final state. In practice this means
    /// COMPLETED/PARTIAL/FAILED/CANCELLED runs (all set `finished_at` via
    /// `set_run_finished`) are included, while REJECTED runs end up
    /// excluded not by any state check but because `reject_run` (a run
    /// rejected while still `PENDING_APPROVAL`, before the scheduler ever
    /// starts it) sets `finished_at` without ever setting `started_at` —
    /// the existing `started_at IS NOT NULL` predicate alone correctly
    /// keeps a run that never actually ran out of a *duration* average, no
    /// special-casing added. A run still in flight (`finished_at IS NULL`)
    /// is excluded the same way, never treated as zero duration. `None`
    /// when no run qualifies yet — an honest "no data", never a fabricated
    /// `0` (same BE-12 data-integrity rule the task-duration query above
    /// already follows).
    pub async fn dashboard_avg_run_duration_ms(&self, project_id: &str) -> Result<Option<f64>> {
        let row = sqlx::query(
            "SELECT AVG((julianday(r.finished_at) - julianday(r.started_at)) * 86400000.0) as avg_ms
             FROM runs r
             WHERE r.project_id = ? AND r.started_at IS NOT NULL AND r.finished_at IS NOT NULL",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("avg_ms"))
    }

    pub async fn get_run_plan(&self, run_id: RunId) -> Result<Option<RunPlan>> {
        let row = sqlx::query("SELECT plan FROM runs WHERE id = ?")
            .bind(run_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        Ok(match row {
            Some(r) => Some(serde_json::from_str(&r.get::<String, _>("plan"))?),
            None => None,
        })
    }

    /// Runs recovery needs to look at on startup: either the RUN row itself
    /// is non-terminal (the process died mid-run — crash, kill, host
    /// restart), or a task under it is still QUEUED/RUNNING even though the
    /// run row is already terminal.
    ///
    /// The second half is D3: a mid-run durable-write failure can make the
    /// live driver mark the *run* FAILED (B1/D2's escalation, correctly)
    /// while the *task's own* terminal-status write was the very write that
    /// failed — leaving that one task stuck at RUNNING forever, because the
    /// run row no longer qualifies as "incomplete" by itself. The same
    /// invariant `recover_incomplete_runs`'s doc comment already states —
    /// "any run/task the DB shows as non-terminal at startup must have been
    /// orphaned by an unclean shutdown" — applies per-task, not just
    /// per-run, so this widens the selection instead of adding a second
    /// recovery code path.
    pub async fn list_incomplete_runs(&self) -> Result<Vec<Uuid>> {
        let rows = sqlx::query(
            "SELECT id FROM runs WHERE state IN ('CREATED','SCHEDULED','RUNNING','PENDING_APPROVAL')
             UNION
             SELECT DISTINCT run_id AS id FROM tasks WHERE status IN ('QUEUED','RUNNING')",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| Uuid::parse_str(&r.get::<String, _>("id")).ok())
            .collect())
    }

    // ------------------------------------------------------ custom skills

    /// Persist a real custom skill so it survives a reload/restart — the
    /// same `description` a caller later selects for generation is what
    /// `ai_generation.rs` embeds verbatim into the AI prompt, so this row is
    /// the entire durable state a custom skill needs.
    pub async fn insert_custom_skill(
        &self,
        id: &str,
        name: &str,
        description: &str,
        source_file: Option<&str>,
        metadata: Option<&serde_json::Value>,
        created_by: &str,
    ) -> Result<()> {
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO custom_skills (id, name, description, source_file, metadata, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(source_file)
        .bind(metadata.map(|m| m.to_string()))
        .bind(created_by)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// All persisted custom skills, oldest first — same ordering convention
    /// as `list_projects`. No project scoping: like `ai_settings`, a skill
    /// is a generation-scope configuration value shared platform-wide, not
    /// owned by one project.
    pub async fn list_custom_skills(&self) -> Result<Vec<CustomSkillRow>> {
        let rows = sqlx::query(
            "SELECT id, name, description, source_file, metadata, created_by, created_at FROM custom_skills ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(custom_skill_from_row).collect())
    }

    /// Returns `false` (no row deleted) when `id` does not exist, so the
    /// caller can return 404 rather than a false "ok".
    pub async fn delete_custom_skill(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM custom_skills WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ------------------------------------------------- custom AI providers

    /// Persist a real, user-added AI provider (see migration
    /// `0013_custom_ai_providers.sql`'s own doc comment for why `api_key` is
    /// plaintext here, not the one-way `key_hash` this app's own auth keys
    /// use). Platform-wide like `ai_settings`/`custom_skills` — no
    /// `project_id`. `prices` is the provider's (input, output) USD per 1,000
    /// tokens when the user entered them, `None` otherwise (see migration
    /// `0019_custom_provider_pricing.sql`).
    pub async fn insert_custom_ai_provider(
        &self,
        id: &str,
        name: &str,
        base_url: &str,
        api_key: &str,
        prices: Option<(f64, f64)>,
        created_by: &str,
    ) -> Result<()> {
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO custom_ai_providers (id, name, base_url, api_key, input_usd_per_1k, output_usd_per_1k, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(base_url)
        .bind(api_key)
        .bind(prices.map(|p| p.0))
        .bind(prices.map(|p| p.1))
        .bind(created_by)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// All persisted custom AI providers, oldest first — same ordering
    /// convention as `list_custom_skills`. Includes the real `api_key` in
    /// full (this is the one internal read path that genuinely needs it —
    /// to actually authenticate real outbound requests, and to rebuild
    /// `AiInfrastructure`'s in-memory set on startup/after a mutation); the
    /// real HTTP-facing list endpoint (`api.rs::list_custom_ai_providers_handler`)
    /// never serializes this row directly, so the raw key never reaches a
    /// client response.
    pub async fn list_custom_ai_providers(&self) -> Result<Vec<CustomAiProviderRow>> {
        let rows = sqlx::query(
            "SELECT id, name, base_url, api_key, input_usd_per_1k, output_usd_per_1k, created_by, created_at FROM custom_ai_providers ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(custom_ai_provider_from_row).collect())
    }

    /// Returns `false` (no row deleted) when `id` does not exist, so the
    /// caller can return 404 rather than a false "ok".
    pub async fn delete_custom_ai_provider(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM custom_ai_providers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ----------------------------------------------------------- projects

    /// BE-8 (API Contract §5.2). `id` is generated by the caller
    /// (`Uuid::new_v4()`), matching every other entity in this store
    /// (`evidence`, `bug_drafts`, `api_keys`) rather than relying on a
    /// DB-side default.
    pub async fn insert_project(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<()> {
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO projects (id, name, description, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(name)
        .bind(description)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// BE-8 (API Contract §5.1). `project_filter`: `None` returns every
    /// project (an unscoped/global key); `Some(pid)` returns at most the one
    /// project matching that id — the same `Option<&str>` filter convention
    /// already used by `list_bug_drafts`/`list_test_intelligence`/
    /// `task_history_by_name`, so a project-scoped caller's "list" is just
    /// its own project (or empty, if that project id was never actually
    /// created via `insert_project`).
    pub async fn list_projects(&self, project_filter: Option<&str>) -> Result<Vec<Project>> {
        let rows = match project_filter {
            Some(pid) => {
                sqlx::query("SELECT id, name, description, created_at, updated_at FROM projects WHERE id = ? ORDER BY created_at")
                    .bind(pid)
                    .fetch_all(&self.pool)
                    .await?
            }
            None => {
                sqlx::query("SELECT id, name, description, created_at, updated_at FROM projects ORDER BY created_at")
                    .fetch_all(&self.pool)
                    .await?
            }
        };
        Ok(rows.into_iter().map(project_from_row).collect())
    }

    /// BE-8 (API Contract §5.3). Single-project lookup, used both for
    /// `GET .../projects/{id}` and as the existence check before `PATCH`.
    pub async fn get_project(&self, id: Uuid) -> Result<Option<Project>> {
        let row = sqlx::query(
            "SELECT id, name, description, created_at, updated_at FROM projects WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(project_from_row))
    }

    /// BE-8 (API Contract §5.4). Partial update: `None` leaves that field
    /// unchanged (`COALESCE`), matching the same pattern `set_run_finished`
    /// already uses for `policy_notes`. Returns `false` (zero rows affected)
    /// when `id` does not exist, so the caller can return 404 rather than a
    /// false "ok" on a no-op update.
    pub async fn update_project(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE projects SET name = COALESCE(?, name), description = COALESCE(?, description), updated_at = ? WHERE id = ?",
        )
        .bind(name)
        .bind(description)
        .bind(now_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Deletes a project and all associated project-scoped data atomically.
    /// Returns `true` if deleted, `false` if the project was not found.
    pub async fn delete_project(&self, id: Uuid) -> Result<bool> {
        // Reads before it writes, so the write lock is taken at BEGIN (see `begin_write`).
        let mut tx = self.begin_write().await?;
        let pid_str = id.to_string();

        let exists = sqlx::query("SELECT id FROM projects WHERE id = ?")
            .bind(&pid_str)
            .fetch_optional(&mut *tx)
            .await?;

        if exists.is_none() {
            return Ok(false);
        }

        // 1. Delete test cases
        sqlx::query("DELETE FROM test_cases WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 1b. Delete test suites and test plans (Phase 5) — real, direct
        // deletes, not left to `ON DELETE SET NULL` alone: that clause
        // exists so removing ONE plan/suite detaches its contents without
        // destroying them, but a whole-project delete must remove every
        // plan/suite row that belonged to this project, not just null out
        // references from rows already deleted above.
        sqlx::query("DELETE FROM test_suites WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM test_plans WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 2. Delete performance runs
        sqlx::query("DELETE FROM performance_runs WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 3. Delete task_dependencies for runs in this project
        sqlx::query("DELETE FROM task_dependencies WHERE run_id IN (SELECT id FROM runs WHERE project_id = ?)")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 4. Delete runs (cascades tasks, run_events, evidence)
        sqlx::query("DELETE FROM runs WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 5. Delete bug_drafts for failure_clusters in this project
        sqlx::query("DELETE FROM bug_drafts WHERE failure_cluster_id IN (SELECT id FROM failure_clusters WHERE project_id = ?)")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 6. Delete failure_clusters
        sqlx::query("DELETE FROM failure_clusters WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 7. Delete quality_nodes (cascades quality_relations, risk_assessments, test_value_assessments, flakiness_metrics, change_impacts)
        sqlx::query("DELETE FROM quality_nodes WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 8. Delete api_keys scoped to this project
        sqlx::query("DELETE FROM api_keys WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 8b. Delete test_case_embeddings scoped to this project — a
        // pre-existing gap this Phase 2 change also closes: no FK exists to
        // cascade this (`0014_test_case_embeddings.sql`'s own comment), and
        // `delete_test_case` only ever cleans up one case's row at a time,
        // so a whole-project delete previously left every embedded case's
        // row behind as a real orphan.
        sqlx::query("DELETE FROM test_case_embeddings WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 8c. Delete documents (Phase 2 — document-level RAG) scoped to this
        // project. Same no-FK convention as every other project-scoped
        // table here (`0004_projects.sql`'s own comment).
        sqlx::query("DELETE FROM documents WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 8d. Delete requirement documents and the requirements read out of
        // them (Phase 3) — no FK on `project_id`, same convention as above.
        sqlx::query("DELETE FROM requirements WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM requirement_documents WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 8e. Delete autopilot runs with their steps and events (Phase 7) — children first, so this does not depend
        // on the foreign keys' cascade being switched on.
        sqlx::query("DELETE FROM autopilot_events WHERE run_id IN (SELECT id FROM autopilot_runs WHERE project_id = ?)")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM autopilot_steps WHERE run_id IN (SELECT id FROM autopilot_runs WHERE project_id = ?)")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM autopilot_runs WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 8f. Delete the API tests made from OpenAPI documents (Phase 8B). No foreign key holds them: `run_id` is only the run's id kept
        // as text, and the runs themselves are deleted above.
        sqlx::query("DELETE FROM openapi_runs WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 8g. Delete scheduled regression (Phase 8C): the runs of each schedule, then the schedules. No foreign keys, as above.
        sqlx::query("DELETE FROM regression_runs WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM regression_schedules WHERE project_id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        // 9. Delete the project itself
        let result = sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(&pid_str)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    // -------------------------------------------------------------- AI settings
    //
    // Central AI Provider & Model Intelligence Layer's persisted routing
    // settings — one real, singleton row (`id = 'global'`). `get`/`upsert`
    // are the only two operations this needs; there is nothing to list or
    // delete.

    /// The 5th element is the real, raw JSON text of
    /// `ai_router.rs::AiSettings::task_overrides` (`None` when no overrides
    /// are configured) — the caller deserializes it, this layer only moves
    /// real bytes in and out of the real column.
    pub async fn get_ai_settings(
        &self,
    ) -> Result<Option<(String, Option<String>, Option<String>, bool, Option<String>)>> {
        let row = sqlx::query("SELECT mode, preferred_provider, preferred_model, fallback_enabled, task_overrides FROM ai_settings WHERE id = 'global'")
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| {
            let mode: String = r.get("mode");
            let preferred_provider: Option<String> = r.get("preferred_provider");
            let preferred_model: Option<String> = r.get("preferred_model");
            let fallback_enabled: i64 = r.get("fallback_enabled");
            let task_overrides: Option<String> = r.get("task_overrides");
            (
                mode,
                preferred_provider,
                preferred_model,
                fallback_enabled != 0,
                task_overrides,
            )
        }))
    }

    /// `task_overrides` is the real, pre-serialized JSON text of
    /// `AiSettings::task_overrides` (`None` to persist "no overrides").
    pub async fn upsert_ai_settings(
        &self,
        mode: &str,
        preferred_provider: Option<&str>,
        preferred_model: Option<&str>,
        fallback_enabled: bool,
        task_overrides: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO ai_settings (id, mode, preferred_provider, preferred_model, fallback_enabled, task_overrides, updated_at)
             VALUES ('global', ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET mode = excluded.mode, preferred_provider = excluded.preferred_provider,
               preferred_model = excluded.preferred_model, fallback_enabled = excluded.fallback_enabled,
               task_overrides = excluded.task_overrides, updated_at = excluded.updated_at",
        )
        .bind(mode)
        .bind(preferred_provider)
        .bind(preferred_model)
        .bind(fallback_enabled as i64)
        .bind(task_overrides)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ------------------------------------------------- execution display settings

    /// The persisted "show the browser window while a Playwright run executes" preference —
    /// `None` when it was never set (the caller should then use the old default: off/headless,
    /// the only behavior every earlier release had).
    pub async fn get_show_browser_window(&self) -> Result<Option<bool>> {
        let row = sqlx::query(
            "SELECT show_browser_window FROM execution_display_settings WHERE id = 'global'",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            let v: i64 = r.get("show_browser_window");
            v != 0
        }))
    }

    pub async fn set_show_browser_window(&self, show: bool) -> Result<()> {
        sqlx::query(
            "INSERT INTO execution_display_settings (id, show_browser_window, updated_at) VALUES ('global', ?, ?)
             ON CONFLICT(id) DO UPDATE SET show_browser_window = excluded.show_browser_window, updated_at = excluded.updated_at",
        )
        .bind(show as i64)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ------------------------------------------------------------- test cases
    //
    // AUD-005 — see `domain.rs`'s test-case section doc comment for the full
    // lifecycle rationale. `source` is always "generator" today (the only
    // real producer of a `CreateTestCaseRequest` in this codebase); the
    // column exists so a future non-generator source (e.g. a human-authored
    // case) does not need a schema change.

    pub async fn insert_test_case(
        &self,
        project_id: &str,
        req: &CreateTestCaseRequest,
        created_by: Option<&str>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = now_rfc3339();
        let expected_result = req
            .expected_result
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| req.assertions.join("; "));
        sqlx::query(
            "INSERT INTO test_cases (
                id, project_id, title, description, category, priority, severity,
                preconditions, steps, expected_result, assertions, test_data,
                source_document, requirement_reference, source_requirement,
                traceability, risk_factor, tags, source, status,
                created_by, created_at, updated_at, suite_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(project_id)
        .bind(&req.title)
        .bind(&req.description)
        .bind(&req.category)
        .bind(&req.priority)
        .bind(&req.severity)
        .bind(serde_json::to_string(&req.preconditions)?)
        .bind(serde_json::to_string(&req.steps)?)
        .bind(expected_result)
        .bind(serde_json::to_string(&req.assertions)?)
        .bind(&req.test_data)
        .bind(&req.source_document)
        .bind(&req.requirement_reference)
        .bind(&req.source_requirement)
        .bind(req.traceability.as_ref().map(|v| v.to_string()))
        .bind(&req.risk_factor)
        .bind(serde_json::to_string(&req.tags)?)
        .bind("generator")
        .bind(TestCaseStatus::Generated.as_db_str())
        .bind(created_by)
        .bind(&now)
        .bind(&now)
        .bind(req.suite_id.map(|u| u.to_string()))
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn get_test_case(&self, id: Uuid) -> Result<Option<TestCaseRow>> {
        let row = sqlx::query(
            "SELECT id, project_id, title, description, category, priority, severity, \
             preconditions, steps, expected_result, assertions, test_data, source_document, \
             requirement_reference, source_requirement, traceability, risk_factor, tags, source, status, \
             validation_notes, executed_run_id, executed_task_id, created_by, created_at, updated_at, suite_id \
             FROM test_cases WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(test_case_from_row).transpose()
    }

    /// Project-scoped list, optionally narrowed to one lifecycle status
    /// and/or one suite (Phase 5) — same optional-filter shape
    /// `list_projects`/`list_test_intelligence` already use, extended to a
    /// second independent filter rather than a second function. Newest
    /// first, matching every other list endpoint. Four literal query
    /// strings, not one built by `format!`: sqlx 0.9 requires a real
    /// `&'static str` for `query()` specifically so dynamic SQL can't
    /// silently slip through (`SqlSafeStr` is only implemented for
    /// `&'static str`, confirmed at compile time, not just a style
    /// preference — same reason `list_bug_drafts` above is two literal
    /// statements rather than one built by `format!`).
    pub async fn list_test_cases(
        &self,
        project_id: &str,
        status_filter: Option<&str>,
        suite_id_filter: Option<Uuid>,
    ) -> Result<Vec<TestCaseRow>> {
        let rows = match (status_filter, suite_id_filter) {
            (Some(status), Some(suite_id)) => {
                sqlx::query(
                    "SELECT id, project_id, title, description, category, priority, severity, \
                     preconditions, steps, expected_result, assertions, test_data, source_document, \
                     requirement_reference, source_requirement, traceability, risk_factor, tags, source, status, \
                     validation_notes, executed_run_id, executed_task_id, created_by, created_at, updated_at, suite_id \
                     FROM test_cases WHERE project_id = ? AND status = ? AND suite_id = ? ORDER BY created_at DESC LIMIT ?",
                )
                .bind(project_id)
                .bind(status)
                .bind(suite_id.to_string())
                .bind(LIST_TEST_CASES_MAX)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(status), None) => {
                sqlx::query(
                    "SELECT id, project_id, title, description, category, priority, severity, \
                     preconditions, steps, expected_result, assertions, test_data, source_document, \
                     requirement_reference, source_requirement, traceability, risk_factor, tags, source, status, \
                     validation_notes, executed_run_id, executed_task_id, created_by, created_at, updated_at, suite_id \
                     FROM test_cases WHERE project_id = ? AND status = ? ORDER BY created_at DESC LIMIT ?",
                )
                .bind(project_id)
                .bind(status)
                .bind(LIST_TEST_CASES_MAX)
                .fetch_all(&self.pool)
                .await?
            }
            (None, Some(suite_id)) => {
                sqlx::query(
                    "SELECT id, project_id, title, description, category, priority, severity, \
                     preconditions, steps, expected_result, assertions, test_data, source_document, \
                     requirement_reference, source_requirement, traceability, risk_factor, tags, source, status, \
                     validation_notes, executed_run_id, executed_task_id, created_by, created_at, updated_at, suite_id \
                     FROM test_cases WHERE project_id = ? AND suite_id = ? ORDER BY created_at DESC LIMIT ?",
                )
                .bind(project_id)
                .bind(suite_id.to_string())
                .bind(LIST_TEST_CASES_MAX)
                .fetch_all(&self.pool)
                .await?
            }
            (None, None) => {
                sqlx::query(
                    "SELECT id, project_id, title, description, category, priority, severity, \
                     preconditions, steps, expected_result, assertions, test_data, source_document, \
                     requirement_reference, source_requirement, traceability, risk_factor, tags, source, status, \
                     validation_notes, executed_run_id, executed_task_id, created_by, created_at, updated_at, suite_id \
                     FROM test_cases WHERE project_id = ? ORDER BY created_at DESC LIMIT ?",
                )
                .bind(project_id)
                .bind(LIST_TEST_CASES_MAX)
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(test_case_from_row).collect()
    }

    /// Partial update of the still-editable descriptive fields. Deliberately
    /// does not allow moving `status` from here — that only ever happens
    /// through the dedicated, conditional transition methods below, never a
    /// generic PATCH (same separation `set_run_finished` vs. a hypothetical
    /// "PATCH run" already keeps for runs).
    #[allow(clippy::too_many_arguments)]
    pub async fn update_test_case(
        &self,
        id: Uuid,
        title: Option<&str>,
        description: Option<&str>,
        priority: Option<&str>,
        preconditions: Option<&Vec<String>>,
        steps: Option<&Vec<TestStep>>,
        expected_result: Option<&str>,
    ) -> Result<bool> {
        let preconditions_json = preconditions.map(serde_json::to_string).transpose()?;
        let steps_json = steps.map(serde_json::to_string).transpose()?;
        let result = sqlx::query(
            "UPDATE test_cases SET
                title = COALESCE(?, title),
                description = COALESCE(?, description),
                priority = COALESCE(?, priority),
                preconditions = COALESCE(?, preconditions),
                steps = COALESCE(?, steps),
                expected_result = COALESCE(?, expected_result),
                updated_at = ?
             WHERE id = ?",
        )
        .bind(title)
        .bind(description)
        .bind(priority)
        .bind(preconditions_json)
        .bind(steps_json)
        .bind(expected_result)
        .bind(now_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Transactional (Vector Store v1 added a second real row this must
    /// also clean up — same transaction pattern `delete_project`
    /// already uses for its own multi-table delete, not a bare
    /// single-statement `DELETE` anymore): a test case's own
    /// `test_case_embeddings` row (if any — most cases never reach
    /// APPROVED and so never had one) is deleted first, then the case
    /// itself. No FK/`ON DELETE CASCADE` exists between the two tables
    /// (see `0014_test_case_embeddings.sql`'s own comment for why), so
    /// this explicit two-step delete is the one place that would
    /// otherwise silently orphan an embedding row forever.
    pub async fn delete_test_case(&self, id: Uuid) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let id_str = id.to_string();
        sqlx::query("DELETE FROM test_case_embeddings WHERE test_case_id = ?")
            .bind(&id_str)
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("DELETE FROM test_cases WHERE id = ?")
            .bind(&id_str)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    // ------------------------------------------- test case embeddings (RAG v1)

    /// Real `(re)INSERT OR REPLACE` — a case embedded once on approval and
    /// then edited while still approved gets a fresh row here each time
    /// (`api.rs::embed_approved_test_case`'s own doc comment), never a
    /// second stale one under the same `test_case_id` (the real `PRIMARY
    /// KEY`).
    pub async fn upsert_test_case_embedding(
        &self,
        test_case_id: Uuid,
        project_id: &str,
        model: &str,
        dimensions: u32,
        embedding: &[f32],
    ) -> Result<()> {
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO test_case_embeddings (test_case_id, project_id, model, dimensions, embedding, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(test_case_id) DO UPDATE SET
                project_id = excluded.project_id,
                model = excluded.model,
                dimensions = excluded.dimensions,
                embedding = excluded.embedding,
                updated_at = excluded.updated_at",
        )
        .bind(test_case_id.to_string())
        .bind(project_id)
        .bind(model)
        .bind(dimensions as i64)
        .bind(serde_json::to_string(embedding)?)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Every real embedded case in one project — the real candidate pool
    /// `ai_generation::rank_similar_test_cases` brute-force-scores against
    /// a generation request's own query embedding. Unordered (ranking is
    /// the caller's job, not this read's).
    pub async fn list_test_case_embeddings(
        &self,
        project_id: &str,
    ) -> Result<Vec<TestCaseEmbeddingRow>> {
        let rows = sqlx::query(
            "SELECT test_case_id, embedding FROM test_case_embeddings WHERE project_id = ? \
             ORDER BY test_case_id",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(test_case_embedding_from_row).collect()
    }

    /// Real count for `get_neural_pipeline`'s "Vector Store" stage metric —
    /// how many of this project's test cases genuinely have a real
    /// embedding right now, not a claim about the project's total case
    /// count.
    pub async fn count_test_case_embeddings(&self, project_id: &str) -> Result<i64> {
        let row =
            sqlx::query("SELECT COUNT(*) as c FROM test_case_embeddings WHERE project_id = ?")
                .bind(project_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(row.get::<i64, _>("c"))
    }

    // ------------------------------------------------- documents (Phase 2 RAG)

    /// Phase 2 (document-level RAG). Real `(re)INSERT OR UPDATE` keyed by
    /// `(project_id, content_hash)` — see `0017_documents.sql`'s own comment
    /// for why this specific key. A re-upload of the exact same extracted
    /// content only refreshes `file_name`/`char_count`/`truncated`/
    /// `updated_at` on the existing row (never touches its `id`, so any
    /// embedding already stored under that id stays valid and does not need
    /// recomputing for unchanged content) rather than creating a second,
    /// duplicate row. Returns the row's real id — freshly generated for a
    /// genuinely new document, or the pre-existing row's own id for a
    /// re-upload — via a follow-up `SELECT` rather than SQLite's `RETURNING`
    /// clause (not used anywhere else in this codebase yet, so this stays
    /// consistent with every other upsert here).
    pub async fn upsert_document(
        &self,
        project_id: &str,
        file_name: &str,
        content_hash: &str,
        text: &str,
        char_count: i64,
        truncated: bool,
    ) -> Result<Uuid> {
        let now = now_rfc3339();
        let new_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO documents (id, project_id, file_name, content_hash, text, char_count, truncated, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(project_id, content_hash) DO UPDATE SET
                file_name = excluded.file_name,
                char_count = excluded.char_count,
                truncated = excluded.truncated,
                updated_at = excluded.updated_at",
        )
        .bind(new_id.to_string())
        .bind(project_id)
        .bind(file_name)
        .bind(content_hash)
        .bind(text)
        .bind(char_count)
        .bind(truncated as i64)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query("SELECT id FROM documents WHERE project_id = ? AND content_hash = ?")
            .bind(project_id)
            .bind(content_hash)
            .fetch_one(&self.pool)
            .await?;
        Ok(Uuid::parse_str(&row.get::<String, _>("id"))?)
    }

    /// Phase 2 (document-level RAG). Sets/refreshes one document's real
    /// embedding after a successful `state.embedder.embed` call
    /// (`api.rs::persist_and_embed_document`) — kept separate from
    /// `upsert_document` because a document's text is always stored
    /// immediately, even before an embedding provider is configured or a
    /// real embed call for it has succeeded (fail-open, same contract
    /// `embed_approved_test_case` already holds itself to).
    pub async fn set_document_embedding(
        &self,
        document_id: Uuid,
        model: &str,
        dimensions: u32,
        embedding: &[f32],
    ) -> Result<()> {
        sqlx::query(
            "UPDATE documents SET embedding = ?, embedding_model = ?, embedding_dimensions = ?, updated_at = ? WHERE id = ?",
        )
        .bind(serde_json::to_string(embedding)?)
        .bind(model)
        .bind(dimensions as i64)
        .bind(now_rfc3339())
        .bind(document_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Every real embedded document in one project — the real candidate pool
    /// `ai_generation::rank_similar_documents` brute-force-scores against a
    /// generation request's own query embedding. Unordered (ranking is the
    /// caller's job, not this read's) and excludes any document that has no
    /// embedding yet (no provider configured, or not yet backfilled), same
    /// "only what is genuinely retrievable" contract `list_test_case_
    /// embeddings` already holds.
    pub async fn list_document_embeddings(
        &self,
        project_id: &str,
    ) -> Result<Vec<DocumentEmbeddingRow>> {
        let rows = sqlx::query(
            "SELECT id, embedding FROM documents WHERE project_id = ? AND embedding IS NOT NULL \
             ORDER BY id",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(document_embedding_from_row).collect()
    }

    /// A single document's full real content, fetched after
    /// `rank_similar_documents` has already picked its id — mirrors
    /// `get_test_case`'s own "rank ids first, fetch full rows after" split.
    pub async fn get_document(&self, id: Uuid) -> Result<Option<DocumentRow>> {
        let row = sqlx::query(
            "SELECT id, project_id, file_name, text, char_count, truncated FROM documents WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(document_from_row).transpose()
    }

    /// Real count for `get_neural_pipeline`'s "Vector Store" stage metric —
    /// how many of this project's uploaded documents genuinely have a real
    /// embedding right now, not a claim about how many documents were ever
    /// uploaded.
    pub async fn count_document_embeddings(&self, project_id: &str) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(*) as c FROM documents WHERE project_id = ? AND embedding IS NOT NULL",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>("c"))
    }

    // -------------------------------------------- requirements (Phase 3)

    /// Stores a document's whole extracted text and the requirements
    /// `requirements.rs` read out of it (see `0020_requirements.sql`). Keyed by
    /// `(project_id, content_hash)`: the same content uploaded again refreshes
    /// the file name and type of the existing row and leaves its requirements
    /// alone — unless they were produced by older parsing rules
    /// (`parser_version`), in which case they are replaced. Returns the
    /// document's id either way.
    pub async fn upsert_requirement_document(
        &self,
        project_id: &str,
        file_name: &str,
        doc_type: Option<&str>,
        content_hash: &str,
        text: &str,
        parsed: &crate::requirements::ParsedDocument,
    ) -> Result<Uuid> {
        // Reads before it writes, so the write lock is taken at BEGIN (see `begin_write`).
        let mut tx = self.begin_write().await?;
        let now = now_rfc3339();
        let skipped = serde_json::to_string(&parsed.skipped_sections)?;

        let existing = sqlx::query(
            "SELECT id, parser_version FROM requirement_documents WHERE project_id = ? AND content_hash = ?",
        )
        .bind(project_id)
        .bind(content_hash)
        .fetch_optional(&mut *tx)
        .await?;

        let (id, rewrite) = match existing {
            Some(row) => {
                let id: String = row.get("id");
                let version: i64 = row.get("parser_version");
                sqlx::query(
                    "UPDATE requirement_documents SET file_name = ?, doc_type = ?, updated_at = ? WHERE id = ?",
                )
                .bind(file_name)
                .bind(doc_type)
                .bind(&now)
                .bind(&id)
                .execute(&mut *tx)
                .await?;
                (id, version != crate::requirements::PARSER_VERSION)
            }
            None => {
                let id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO requirement_documents (id, project_id, file_name, doc_type, content_hash, char_count, text, parser_version, sections_count, skipped_sections, furniture_lines, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(project_id)
                .bind(file_name)
                .bind(doc_type)
                .bind(content_hash)
                .bind(text.chars().count() as i64)
                .bind(text)
                .bind(crate::requirements::PARSER_VERSION)
                .bind(parsed.sections_count as i64)
                .bind(&skipped)
                .bind(parsed.furniture_lines as i64)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
                (id, true)
            }
        };

        if rewrite {
            sqlx::query(
                "UPDATE requirement_documents SET parser_version = ?, sections_count = ?, skipped_sections = ?, furniture_lines = ? WHERE id = ?",
            )
            .bind(crate::requirements::PARSER_VERSION)
            .bind(parsed.sections_count as i64)
            .bind(&skipped)
            .bind(parsed.furniture_lines as i64)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM requirements WHERE document_id = ?")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            for r in &parsed.requirements {
                sqlx::query(
                    "INSERT INTO requirements (id, project_id, document_id, req_key, ordinal, section, kind, text)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(project_id)
                .bind(&id)
                .bind(&r.key)
                .bind(r.ordinal as i64)
                .bind(r.section.as_deref())
                .bind(r.kind.as_str())
                .bind(&r.text)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
        Ok(Uuid::parse_str(&id)?)
    }

    /// A project's requirement documents in the order they were first
    /// uploaded, each with how many requirements it holds.
    pub async fn list_requirement_documents(
        &self,
        project_id: &str,
    ) -> Result<Vec<RequirementDocumentRow>> {
        let rows = sqlx::query(
            "SELECT d.id, d.file_name, d.doc_type, d.char_count, d.sections_count, d.skipped_sections,
                    d.furniture_lines, d.created_at, d.updated_at,
                    (SELECT COUNT(*) FROM requirements r WHERE r.document_id = d.id) AS requirements_count
             FROM requirement_documents d WHERE d.project_id = ? ORDER BY d.created_at, d.id",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(requirement_document_from_row)
            .collect()
    }

    /// The requirements of `document_ids` in that order (each document's own
    /// in document order); every document of the project, oldest first, when
    /// `document_ids` is empty. A document that belongs to another project
    /// simply contributes nothing.
    ///
    /// This used to issue one query *per* document id, so asking for a project's
    /// N requirement documents cost N round trips. It is now a single
    /// `IN (...)` query, with the caller's ordering expressed as a `CASE
    /// document_id WHEN ... THEN <position>` sort key so each document's rows
    /// still come back grouped in the order they were asked for and in document
    /// order within it. `QueryBuilder` rather than a `format!`ed string, for the
    /// reason `create_run`'s own bulk insert gives (see the note there): the
    /// number of bind placeholders is only known at runtime here.
    pub async fn list_requirements(
        &self,
        project_id: &str,
        document_ids: &[Uuid],
    ) -> Result<Vec<RequirementRow>> {
        if document_ids.is_empty() {
            let rows = sqlx::query(
                "SELECT r.document_id, r.req_key, r.ordinal, r.section, r.kind, r.text
                 FROM requirements r JOIN requirement_documents d ON d.id = r.document_id
                 WHERE r.project_id = ? ORDER BY d.created_at, d.id, r.ordinal",
            )
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;
            return rows.into_iter().map(requirement_from_row).collect();
        }
        let ids: Vec<String> = document_ids.iter().map(|id| id.to_string()).collect();
        let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
            "SELECT document_id, req_key, ordinal, section, kind, text
             FROM requirements WHERE project_id = ",
        );
        qb.push_bind(project_id.to_string());
        qb.push(" AND document_id IN (");
        {
            let mut separated = qb.separated(", ");
            for id in &ids {
                separated.push_bind(id.clone());
            }
        }
        qb.push(") ORDER BY CASE document_id");
        for (position, id) in ids.iter().enumerate() {
            qb.push(" WHEN ")
                .push_bind(id.clone())
                .push(" THEN ")
                .push_bind(position as i64);
        }
        // No document in the table can fall outside the list above (the `IN`
        // already restricted to it), so `ELSE` is unreachable; it exists only
        // because `CASE` needs a final `ELSE` to be well-formed.
        qb.push(" ELSE ").push_bind(ids.len() as i64);
        qb.push(" END, ordinal");
        let rows = qb.build().fetch_all(&self.pool).await?;
        rows.into_iter().map(requirement_from_row).collect()
    }

    /// Phase 5 (Output maturity) — the one place a case's suite membership
    /// is set OR cleared. Unlike `update_test_case`'s `COALESCE` fields,
    /// `suite_id` here is a real, direct assignment: `Some(id)` sets it,
    /// `None` clears it back to ungrouped. See `domain.rs::
    /// SetTestCaseSuiteRequest`'s own doc comment for why this needs to be
    /// its own method/endpoint rather than a third `COALESCE` field.
    pub async fn set_test_case_suite(&self, id: Uuid, suite_id: Option<Uuid>) -> Result<bool> {
        let result = sqlx::query("UPDATE test_cases SET suite_id = ?, updated_at = ? WHERE id = ?")
            .bind(suite_id.map(|u| u.to_string()))
            .bind(now_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ------------------------------------------------- test plans (Phase 5)

    pub async fn insert_test_plan(
        &self,
        project_id: &str,
        req: &CreateTestPlanRequest,
        created_by: Option<&str>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO test_plans (id, project_id, name, description, status, entry_criteria, exit_criteria, created_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(project_id)
        .bind(&req.name)
        .bind(&req.description)
        .bind(TestPlanStatus::Draft.as_db_str())
        .bind(&req.entry_criteria)
        .bind(&req.exit_criteria)
        .bind(created_by)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn get_test_plan(&self, id: Uuid) -> Result<Option<TestPlanRow>> {
        let row = sqlx::query(
            "SELECT p.id, p.project_id, p.name, p.description, p.status, p.entry_criteria, p.exit_criteria, \
             p.created_by, p.created_at, p.updated_at, \
             (SELECT COUNT(*) FROM test_suites s WHERE s.plan_id = p.id) as suite_count \
             FROM test_plans p WHERE p.id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(test_plan_from_row).transpose()
    }

    pub async fn list_test_plans(&self, project_id: &str) -> Result<Vec<TestPlanRow>> {
        let rows = sqlx::query(
            "SELECT p.id, p.project_id, p.name, p.description, p.status, p.entry_criteria, p.exit_criteria, \
             p.created_by, p.created_at, p.updated_at, \
             (SELECT COUNT(*) FROM test_suites s WHERE s.plan_id = p.id) as suite_count \
             FROM test_plans p WHERE p.project_id = ? ORDER BY p.created_at DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(test_plan_from_row).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_test_plan(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        status: Option<&str>,
        entry_criteria: Option<&str>,
        exit_criteria: Option<&str>,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE test_plans SET
                name = COALESCE(?, name),
                description = COALESCE(?, description),
                status = COALESCE(?, status),
                entry_criteria = COALESCE(?, entry_criteria),
                exit_criteria = COALESCE(?, exit_criteria),
                updated_at = ?
             WHERE id = ?",
        )
        .bind(name)
        .bind(description)
        .bind(status)
        .bind(entry_criteria)
        .bind(exit_criteria)
        .bind(now_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Real, direct deletion — `test_suites.plan_id` detaches
    /// (`ON DELETE SET NULL`, see 0011_test_plan_suite_hierarchy.sql's own
    /// comment) rather than cascading: removing a plan must never silently
    /// delete the real suites/cases that were inside it.
    pub async fn delete_test_plan(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM test_plans WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ------------------------------------------------ test suites (Phase 5)

    pub async fn insert_test_suite(
        &self,
        project_id: &str,
        req: &CreateTestSuiteRequest,
        created_by: Option<&str>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO test_suites (id, project_id, plan_id, name, description, created_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(project_id)
        .bind(req.plan_id.map(|u| u.to_string()))
        .bind(&req.name)
        .bind(&req.description)
        .bind(created_by)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn get_test_suite(&self, id: Uuid) -> Result<Option<TestSuiteRow>> {
        let row = sqlx::query(
            "SELECT s.id, s.project_id, s.plan_id, s.name, s.description, s.created_by, s.created_at, s.updated_at, \
             (SELECT COUNT(*) FROM test_cases c WHERE c.suite_id = s.id) as case_count \
             FROM test_suites s WHERE s.id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(test_suite_from_row).transpose()
    }

    /// Project-scoped list, optionally narrowed to one plan — same
    /// optional-filter shape `list_test_cases` above uses.
    pub async fn list_test_suites(
        &self,
        project_id: &str,
        plan_id_filter: Option<Uuid>,
    ) -> Result<Vec<TestSuiteRow>> {
        let rows = match plan_id_filter {
            Some(plan_id) => {
                sqlx::query(
                    "SELECT s.id, s.project_id, s.plan_id, s.name, s.description, s.created_by, s.created_at, s.updated_at, \
                     (SELECT COUNT(*) FROM test_cases c WHERE c.suite_id = s.id) as case_count \
                     FROM test_suites s WHERE s.project_id = ? AND s.plan_id = ? ORDER BY s.created_at DESC",
                )
                .bind(project_id)
                .bind(plan_id.to_string())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT s.id, s.project_id, s.plan_id, s.name, s.description, s.created_by, s.created_at, s.updated_at, \
                     (SELECT COUNT(*) FROM test_cases c WHERE c.suite_id = s.id) as case_count \
                     FROM test_suites s WHERE s.project_id = ? ORDER BY s.created_at DESC",
                )
                .bind(project_id)
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(test_suite_from_row).collect()
    }

    pub async fn update_test_suite(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE test_suites SET name = COALESCE(?, name), description = COALESCE(?, description), updated_at = ? WHERE id = ?",
        )
        .bind(name)
        .bind(description)
        .bind(now_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// The one place a suite's plan membership is set OR cleared — same
    /// "real direct assignment, not `COALESCE`" shape as
    /// `set_test_case_suite` above, and for the same reason (see
    /// `domain.rs::SetTestSuitePlanRequest`'s own doc comment).
    pub async fn set_test_suite_plan(&self, id: Uuid, plan_id: Option<Uuid>) -> Result<bool> {
        let result = sqlx::query("UPDATE test_suites SET plan_id = ?, updated_at = ? WHERE id = ?")
            .bind(plan_id.map(|u| u.to_string()))
            .bind(now_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Real, direct deletion — `test_cases.suite_id` detaches
    /// (`ON DELETE SET NULL`) rather than cascading, same reasoning as
    /// `delete_test_plan` above.
    pub async fn delete_test_suite(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM test_suites WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// `domain.rs`'s own doc comment on `TestCaseStatus`: every transition
    /// below is a conditional `UPDATE ... WHERE status = <expected prior
    /// state(s)>`, not just checked in a handler after the fact — a race
    /// between two callers (or a stale client retrying an already-applied
    /// transition) can never silently double-apply or skip a step. Zero
    /// rows affected means "the case was not in an eligible state," which
    /// the caller turns into a real 409, not a false "ok". Each query is a
    /// static string (sqlx requires literal SQL for injection-safety), so
    /// each real transition gets its own explicit method rather than one
    /// dynamically-built query.
    pub async fn validate_test_case(&self, id: Uuid, validation_notes: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE test_cases SET status = 'VALIDATED', validation_notes = ?, updated_at = ? WHERE id = ? AND status = 'GENERATED'",
        )
        .bind(validation_notes)
        .bind(now_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn approve_test_case(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("UPDATE test_cases SET status = 'APPROVED', updated_at = ? WHERE id = ? AND status = 'VALIDATED'")
            .bind(now_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Rejectable from either `GENERATED` or `VALIDATED` — not from
    /// `APPROVED`/`EXECUTED`, which have their own, later, real-world
    /// consequences (an approved case may already be mid-handoff).
    pub async fn reject_test_case(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE test_cases SET status = 'REJECTED', updated_at = ? WHERE id = ? AND status IN ('GENERATED', 'VALIDATED')",
        )
        .bind(now_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Handoff: only an `Approved` case may become `Executed`, and only
    /// once — same conditional-`UPDATE` pattern as
    /// `transition_test_case_status`, plus the real run/task ids this case
    /// was actually dispatched as (FRD-RT-04 traceability: Test Case -> Run
    /// -> Task -> Result/Evidence, all already queryable by these two ids
    /// via the existing `runs`/`tasks`/`evidence`/`run_events` tables).
    pub async fn mark_test_case_executed(
        &self,
        id: Uuid,
        run_id: Uuid,
        task_id: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE test_cases SET status = ?, executed_run_id = ?, executed_task_id = ?, updated_at = ?
             WHERE id = ? AND status = ?",
        )
        .bind(TestCaseStatus::Executed.as_db_str())
        .bind(run_id.to_string())
        .bind(task_id)
        .bind(now_rfc3339())
        .bind(id.to_string())
        .bind(TestCaseStatus::Approved.as_db_str())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    // ------------------------------------------------------- performance runs

    /// Real, initial persisted row for a performance test — inserted before
    /// the workload starts (`status = 'RUNNING'`), so a run genuinely
    /// exists in durable storage from the moment it begins, not only once
    /// it finishes. `id` is generated by the caller, same convention as
    /// every other entity in this store.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_performance_run(
        &self,
        id: Uuid,
        project_id: &str,
        name: &str,
        test_type: &str,
        config_json: &str,
        created_by: Option<&str>,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO performance_runs (id, project_id, name, test_type, status, config_json, created_by, started_at)
             VALUES (?, ?, ?, ?, 'RUNNING', ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(project_id)
        .bind(name)
        .bind(test_type)
        .bind(config_json)
        .bind(created_by)
        .bind(started_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Real terminal update — written exactly once, when `run_workload`
    /// actually finishes (or the recovery sweep marks an orphaned row
    /// `INTERRUPTED`). `metrics_json`/`verdict_reasons_json` are `None` only
    /// for the `INTERRUPTED` path, where no real metrics were ever computed.
    #[allow(clippy::too_many_arguments)]
    /// Terminal-state idempotency (Phase 6 ERROR-fix brief §7): guarded by
    /// `WHERE status = 'RUNNING'` so this write only ever takes effect from
    /// the one real non-terminal state a `performance_runs` row starts in.
    /// A run cannot be finalized twice — whichever of `finalize_performance_run`
    /// (normal completion/cancellation) or `finalize_performance_run_as_error`
    /// (a genuine supervised task failure) reaches this first wins, and a
    /// second call for the same run (which should never happen given the two
    /// call sites are mutually exclusive branches of one `JoinHandle` await,
    /// but is guarded here regardless, defensively) is a real, honest no-op
    /// (`rows_affected() == 0`), never a silent overwrite of an
    /// already-terminal ERROR/COMPLETED/CANCELLED result.
    pub async fn finalize_performance_run(
        &self,
        id: Uuid,
        status: &str,
        verdict: Option<&str>,
        metrics_json: Option<&str>,
        verdict_reasons_json: Option<&str>,
        error_reason: Option<&str>,
        evidence_id: Option<Uuid>,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE performance_runs SET status = ?, verdict = ?, metrics_json = ?, verdict_reasons_json = ?, error_reason = ?, evidence_id = ?, finished_at = ?
             WHERE id = ? AND status = 'RUNNING'",
        )
        .bind(status)
        .bind(verdict)
        .bind(metrics_json)
        .bind(verdict_reasons_json)
        .bind(error_reason)
        .bind(evidence_id.map(|u| u.to_string()))
        .bind(now_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_performance_run(&self, id: Uuid) -> Result<Option<PerformanceRunRow>> {
        let row = sqlx::query(
            "SELECT id, project_id, name, test_type, status, verdict, config_json, metrics_json, verdict_reasons_json, error_reason, evidence_id, created_by, started_at, finished_at
             FROM performance_runs WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(performance_run_from_row).transpose()
    }

    /// Project-scoped list, newest first — same shape as `list_test_cases`.
    pub async fn list_performance_runs(&self, project_id: &str) -> Result<Vec<PerformanceRunRow>> {
        let rows = sqlx::query(
            "SELECT id, project_id, name, test_type, status, verdict, config_json, metrics_json, verdict_reasons_json, error_reason, evidence_id, created_by, started_at, finished_at
             FROM performance_runs WHERE project_id = ? ORDER BY started_at DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(performance_run_from_row).collect()
    }

    /// Startup recovery (mirrors `list_incomplete_runs`/`recover_incomplete_runs`
    /// in `engine.rs`): any row still `RUNNING` at process start was
    /// genuinely orphaned by an unclean shutdown — the in-memory workload
    /// handle that was driving it (`AppState::performance_tests`) does not
    /// survive a restart, so nothing will ever finalize that row otherwise.
    pub async fn list_incomplete_performance_runs(&self) -> Result<Vec<Uuid>> {
        let rows = sqlx::query("SELECT id FROM performance_runs WHERE status = 'RUNNING'")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| Uuid::parse_str(&r.get::<String, _>("id")).ok())
            .collect())
    }

    // ------------------------------------------------------------ evidence

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_evidence(
        &self,
        run_id: RunId,
        task_id: &str,
        kind: &str,
        object_key: &str,
        content_type: &str,
        size_bytes: i64,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO evidence (id, run_id, task_id, kind, object_key, content_type, size_bytes, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(run_id.to_string())
        .bind(task_id)
        .bind(kind)
        .bind(object_key)
        .bind(content_type)
        .bind(size_bytes)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn list_evidence_for_run(&self, run_id: RunId) -> Result<Vec<EvidenceRow>> {
        let rows = sqlx::query(
            "SELECT id, task_id, kind, object_key, content_type, size_bytes, created_at FROM evidence WHERE run_id = ? ORDER BY created_at",
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| EvidenceRow {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                task_id: r.get("task_id"),
                kind: r.get("kind"),
                object_key: r.get("object_key"),
                content_type: r.get("content_type"),
                size_bytes: r.get::<i64, _>("size_bytes"),
                created_at: parse_ts(&r.get::<String, _>("created_at")),
            })
            .collect())
    }

    /// Returns (object_key, content_type, run_id) — `run_id` lets the API
    /// layer re-check project ownership before serving the raw content.
    pub async fn get_evidence_object_key(
        &self,
        id: Uuid,
    ) -> Result<Option<(String, Option<String>, Option<String>)>> {
        let row = sqlx::query("SELECT object_key, content_type, run_id FROM evidence WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| (r.get("object_key"), r.get("content_type"), r.get("run_id"))))
    }

    /// BE-15 (§15.2 Evidence Detail) — the metadata row for one evidence
    /// item plus its owning run id (for the project-ownership check every
    /// other scoped resource already does). The real binary content stays
    /// behind the sibling `/content` endpoint (`get_evidence_object_key` +
    /// `evidence.rs`), per the contract's own "large binary artifacts
    /// SHOULD be accessed through secure artifact references rather than
    /// embedded in normal API responses."
    pub async fn get_evidence_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<(EvidenceRow, Option<String>)>> {
        let row = sqlx::query(
            "SELECT id, task_id, kind, object_key, content_type, size_bytes, created_at, run_id FROM evidence WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            let run_id: Option<String> = r.get("run_id");
            (
                EvidenceRow {
                    id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                    task_id: r.get("task_id"),
                    kind: r.get("kind"),
                    object_key: r.get("object_key"),
                    content_type: r.get("content_type"),
                    size_bytes: r.get::<i64, _>("size_bytes"),
                    created_at: parse_ts(&r.get::<String, _>("created_at")),
                },
                run_id,
            )
        }))
    }

    // ----------------------------------------------------------- auth/keys

    pub async fn insert_api_key(
        &self,
        id: Uuid,
        name: &str,
        key_hash: &str,
        role: &str,
        project_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO api_keys (id, name, key_hash, role, project_id, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(name)
        .bind(key_hash)
        .bind(role)
        .bind(project_id)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn count_active_api_keys(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as c FROM api_keys WHERE revoked_at IS NULL")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get("c"))
    }

    pub async fn find_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKeyRow>> {
        let row = sqlx::query(
            "SELECT id, name, role, project_id FROM api_keys WHERE key_hash = ? AND revoked_at IS NULL",
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(api_key_row_from))
    }

    /// The key with this id, if it exists and has not been revoked: what a background job started by a key (the
    /// autopilot) looks up before each call it makes on that key's behalf, so revoking the key stops the job.
    pub async fn find_active_api_key(&self, id: Uuid) -> Result<Option<ApiKeyRow>> {
        let row = sqlx::query(
            "SELECT id, name, role, project_id FROM api_keys WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(api_key_row_from))
    }

    pub async fn list_api_keys(&self) -> Result<Vec<ApiKeyListRow>> {
        let rows = sqlx::query(
            "SELECT id, name, role, project_id, created_at, revoked_at FROM api_keys ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ApiKeyListRow {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                name: r.get("name"),
                role: r.get("role"),
                project_id: r.get("project_id"),
                created_at: parse_ts(&r.get::<String, _>("created_at")),
                revoked_at: parse_ts_opt(r.get("revoked_at")),
            })
            .collect())
    }

    /// Revokes `id`, reporting whether a key was actually changed.
    ///
    /// The `bool` is the number of rows the `UPDATE` matched, and callers must
    /// act on it. This used to return `Result<()>` and discard the row count,
    /// which made `DELETE /admin/api-keys/:id` answer `204 No Content` — and
    /// write an `apikey.revoke` / `status: "ok"` audit row — for a key that
    /// never existed or was already revoked. Live-verified before the fix:
    /// `DELETE /admin/api-keys/00000000-...-0000` returned `204` and the audit
    /// trail then asserted the key had been revoked when nothing had happened.
    /// On an audit log used for compliance, that is a trail-integrity defect,
    /// not just a sloppy status code.
    ///
    /// `false` therefore means "no active key with that id" — either it is not
    /// in the table at all, or it was already revoked. Revoking twice is
    /// genuinely a no-op, so the second revoke is still reported honestly rather
    /// than as a success that changed state.
    pub async fn revoke_api_key(&self, id: Uuid) -> Result<bool> {
        let updated =
            sqlx::query("UPDATE api_keys SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
                .bind(now_rfc3339())
                .bind(id.to_string())
                .execute(&self.pool)
                .await?;
        Ok(updated.rows_affected() > 0)
    }

    // ------------------------------------------------------ quality graph

    /// Upserts a node identifying one test *across runs*. Identity is
    /// `(project_id, node_type, external_ref)` where `external_ref` is the
    /// task's `name` (stable across runs) rather than its `id` (which is
    /// only unique within one run's plan, e.g. "t1"). A bare `NULL`
    /// `project_id` would defeat SQLite's own uniqueness check (two NULLs
    /// never compare equal, so `ON CONFLICT` wouldn't fire and every ingest
    /// would insert a fresh row) — callers pass `"default"` instead of
    /// `None` for untenanted data; see `intelligence.rs`.
    pub async fn upsert_quality_node(
        &self,
        project_id: &str,
        node_type: &str,
        external_ref: &str,
        name: &str,
    ) -> Result<Uuid> {
        let existing = sqlx::query(
            "SELECT id FROM quality_nodes WHERE project_id = ? AND node_type = ? AND external_ref = ?",
        )
        .bind(project_id)
        .bind(node_type)
        .bind(external_ref)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(row) = existing {
            return Ok(Uuid::parse_str(&row.get::<String, _>("id")).unwrap_or_default());
        }
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO quality_nodes (id, project_id, node_type, external_ref, name, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(project_id)
        .bind(node_type)
        .bind(external_ref)
        .bind(name)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// A real edge between two quality-graph nodes — `quality_relations`,
    /// defined in schema since an earlier phase but never written to by any
    /// code path until this one (`intelligence::Intelligence::
    /// link_test_case_to_requirement`, the first real caller — Phase 3
    /// self-learning's own `list_test_intelligence`/`list_bug_drafts` were
    /// reads of pre-existing data; this is the first real write to this
    /// specific table). Upsert on the schema's own real uniqueness
    /// constraint (`from_node_id, to_node_id, relation_type`) — calling
    /// this again for the same real edge is a genuine no-op, not a
    /// duplicate row.
    pub async fn upsert_quality_relation(
        &self,
        from_node_id: Uuid,
        to_node_id: Uuid,
        relation_type: &str,
    ) -> Result<()> {
        let existing = sqlx::query(
            "SELECT id FROM quality_relations WHERE from_node_id = ? AND to_node_id = ? AND relation_type = ?",
        )
        .bind(from_node_id.to_string())
        .bind(to_node_id.to_string())
        .bind(relation_type)
        .fetch_optional(&self.pool)
        .await?;
        if existing.is_some() {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO quality_relations (id, from_node_id, to_node_id, relation_type, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(from_node_id.to_string())
        .bind(to_node_id.to_string())
        .bind(relation_type)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Real requirement-graph impact analysis: every real, persisted test
    /// case reachable from a `requirement` node via a real `validates` edge
    /// — i.e., every test case actually created with this exact
    /// `requirement_key` (a real `requirement_reference` string, or
    /// `"text:<sha256>"` of a real `source_requirement` — see
    /// `intelligence::requirement_identity`). Empty when nothing was ever
    /// linked (no such requirement node, or a node with no real edges to
    /// it yet) — never a guess standing in for a real answer.
    pub async fn list_test_cases_for_requirement(
        &self,
        project_id: &str,
        requirement_key: &str,
    ) -> Result<Vec<TestCaseRow>> {
        let rows = sqlx::query(
            "SELECT tc.id, tc.project_id, tc.title, tc.description, tc.category, tc.priority, tc.severity, \
             tc.preconditions, tc.steps, tc.expected_result, tc.assertions, tc.test_data, tc.source_document, \
             tc.requirement_reference, tc.source_requirement, tc.traceability, tc.risk_factor, tc.tags, tc.source, tc.status, \
             tc.validation_notes, tc.executed_run_id, tc.executed_task_id, tc.created_by, tc.created_at, tc.updated_at, tc.suite_id \
             FROM test_cases tc
             JOIN quality_nodes case_node ON case_node.node_type = 'test_case' AND case_node.external_ref = tc.id AND case_node.project_id = ?
             JOIN quality_relations rel ON rel.from_node_id = case_node.id AND rel.relation_type = 'validates'
             JOIN quality_nodes req_node ON req_node.id = rel.to_node_id AND req_node.node_type = 'requirement' AND req_node.project_id = ?
             WHERE req_node.external_ref = ?
             ORDER BY tc.created_at DESC",
        )
        .bind(project_id)
        .bind(project_id)
        .bind(requirement_key)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(test_case_from_row).collect()
    }

    /// Execution history for every quality-node with this `external_ref`
    /// (i.e. every past run of a test with this name), oldest first. Used to
    /// compute flakiness (status flips) and historical failure rate.
    /// B3 — history feeding risk/flakiness scoring MUST stay inside one
    /// project. This previously filtered on task *name* alone, so two
    /// tenants that happened to name a test the same thing shared a failure
    /// history: a project whose test always passed could be reported at a
    /// 75% failure rate because another tenant's test of the same name kept
    /// failing. `tasks` carries no project_id of its own, so the tenant is
    /// resolved by joining back to the owning run.
    ///
    /// `project_filter` is the resolved tenant key (see
    /// `intelligence::tenant_key`): `Some(pid)` restricts to that tenant,
    /// where an untenanted run counts as `"default"`; `None` means an
    /// unscoped/global caller and returns every tenant's history.
    pub async fn task_history_by_name(
        &self,
        name: &str,
        project_filter: Option<&str>,
        limit: i64,
    ) -> Result<Vec<TaskHistoryRow>> {
        // Two literal statements rather than one built by `format!`: sqlx 0.9
        // rejects runtime-constructed SQL at compile time.
        //
        // The project predicate was `COALESCE(r.project_id, 'default') = ?`,
        // which is non-sargable: the column is wrapped in a function, so
        // `idx_runs_project_created` (migration 0027) cannot be used and the
        // join falls back to a full `SCAN runs`. Rewritten as the explicit
        // disjunction that means exactly the same thing -- this project, or
        // the untenanted bucket when the caller asked for it -- leaving both
        // sides as plain column comparisons. Same fix as `list_bug_drafts`.
        let rows = match project_filter {
            Some(pid) => {
                sqlx::query(
                    "SELECT t.status as status, t.finished_at as finished_at
                     FROM tasks t
                     JOIN runs r ON r.id = t.run_id
                     WHERE t.name = ?1 AND t.finished_at IS NOT NULL
                       AND (r.project_id = ?2
                            OR (r.project_id IS NULL AND ?2 = 'default'))
                     ORDER BY t.finished_at DESC LIMIT ?3",
                )
                .bind(name)
                .bind(pid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT t.status as status, t.finished_at as finished_at
                     FROM tasks t
                     WHERE t.name = ?1 AND t.finished_at IS NOT NULL
                     ORDER BY t.finished_at DESC LIMIT ?2",
                )
                .bind(name)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };
        let mut out: Vec<TaskHistoryRow> = rows
            .into_iter()
            .map(|r| TaskHistoryRow {
                status: TaskStatus::from_db_str(&r.get::<String, _>("status")),
                finished_at: parse_ts(&r.get::<String, _>("finished_at")),
            })
            .collect();
        out.reverse(); // chronological order for flip-counting
        Ok(out)
    }

    /// Result keys of the project's most recent crawls that finished successfully, newest first.
    /// A crawl is a run named `Crawl: <base_url>` holding one task, and that task's id is the key
    /// its result file is stored under (`api.rs::start_crawl_scoped`, `crawler::read_crawl_result`).
    pub async fn recent_completed_crawl_task_ids(
        &self,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<String>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT t.id FROM tasks t
             JOIN runs r ON r.id = t.run_id
             WHERE r.project_id = ? AND r.name LIKE 'Crawl: %'
               AND r.state = 'COMPLETED' AND t.status = 'PASSED'
             ORDER BY r.created_at DESC LIMIT ?",
        )
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(ids)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_risk_assessment(
        &self,
        node_id: Uuid,
        score: f64,
        business_criticality: f64,
        change_frequency: f64,
        historical_failure_rate: f64,
        dependency_count: i64,
        security_sensitivity: f64,
        execution_cost: f64,
        inputs_json: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO risk_assessments (id, node_id, score, business_criticality, change_frequency, historical_failure_rate, dependency_count, security_sensitivity, execution_cost, inputs, computed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(node_id) DO UPDATE SET score=excluded.score, business_criticality=excluded.business_criticality,
               change_frequency=excluded.change_frequency, historical_failure_rate=excluded.historical_failure_rate,
               dependency_count=excluded.dependency_count, security_sensitivity=excluded.security_sensitivity,
               execution_cost=excluded.execution_cost, inputs=excluded.inputs, computed_at=excluded.computed_at",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(node_id.to_string())
        .bind(score)
        .bind(business_criticality)
        .bind(change_frequency)
        .bind(historical_failure_rate)
        .bind(dependency_count)
        .bind(security_sensitivity)
        .bind(execution_cost)
        .bind(inputs_json)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_test_value(
        &self,
        node_id: Uuid,
        value_score: f64,
        risk_coverage: f64,
        historical_defect_detection: f64,
        business_criticality: f64,
        change_relevance: f64,
        execution_cost: f64,
        flakiness: f64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO test_value_assessments (id, node_id, value_score, risk_coverage, historical_defect_detection, business_criticality, change_relevance, execution_cost, flakiness, computed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(node_id) DO UPDATE SET value_score=excluded.value_score, risk_coverage=excluded.risk_coverage,
               historical_defect_detection=excluded.historical_defect_detection, business_criticality=excluded.business_criticality,
               change_relevance=excluded.change_relevance, execution_cost=excluded.execution_cost, flakiness=excluded.flakiness,
               computed_at=excluded.computed_at",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(node_id.to_string())
        .bind(value_score)
        .bind(risk_coverage)
        .bind(historical_defect_detection)
        .bind(business_criticality)
        .bind(change_relevance)
        .bind(execution_cost)
        .bind(flakiness)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_flakiness(
        &self,
        node_id: Uuid,
        flakiness_score: f64,
        window_runs: i64,
        window_failures: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO flakiness_metrics (id, node_id, flakiness_score, window_runs, window_failures, computed_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(node_id) DO UPDATE SET flakiness_score=excluded.flakiness_score, window_runs=excluded.window_runs,
               window_failures=excluded.window_failures, computed_at=excluded.computed_at",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(node_id.to_string())
        .bind(flakiness_score)
        .bind(window_runs)
        .bind(window_failures)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Test-intelligence table: one row per known test node with its latest
    /// risk/value/flakiness, for `GET /intelligence/tests`.
    pub async fn list_test_intelligence(
        &self,
        project_filter: Option<&str>,
    ) -> Result<Vec<TestIntelRow>> {
        let pid = project_filter.unwrap_or("default");
        let rows = sqlx::query(
            "SELECT n.id as id, n.name as name,
                    r.score as risk_score, r.historical_failure_rate as failure_rate, r.dependency_count as dependency_count, r.execution_cost as execution_cost,
                    v.value_score as value_score,
                    f.flakiness_score as flakiness_score, f.window_runs as window_runs, f.window_failures as window_failures
             FROM quality_nodes n
             LEFT JOIN risk_assessments r ON r.node_id = n.id
             LEFT JOIN test_value_assessments v ON v.node_id = n.id
             LEFT JOIN flakiness_metrics f ON f.node_id = n.id
             WHERE n.node_type = 'test' AND n.project_id = ?
             ORDER BY r.score DESC LIMIT ?",
        )
        .bind(pid)
        .bind(LIST_TEST_INTELLIGENCE_MAX)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| TestIntelRow {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                name: r.get("name"),
                risk_score: r.get("risk_score"),
                failure_rate: r.get("failure_rate"),
                dependency_count: r.get("dependency_count"),
                execution_cost_ms: r.get("execution_cost"),
                value_score: r.get("value_score"),
                flakiness_score: r.get("flakiness_score"),
                window_runs: r.get("window_runs"),
                window_failures: r.get("window_failures"),
            })
            .collect())
    }

    // -------------------------------------------------------- failure intel

    /// Upsert-and-increment: a failure with this normalized signature has
    /// been seen again. Returns the cluster id and its post-update
    /// occurrence count (so the caller can decide whether it's crossed the
    /// "recurring, draft a bug" threshold).
    ///
    /// `failure_fingerprint` is the telemetry-side SHA-256 for the same failure, or
    /// `None` when the caller has no telemetry context. Phase 6 (migration 0030)
    /// added the column because a cluster was otherwise unjoinable with the
    /// executions that produced it: the cluster is keyed by normalized text while
    /// telemetry is keyed by a hash, and nothing joined them. It is filled on insert
    /// and backfilled on update **only when still NULL**, so a cluster created before
    /// the migration picks the value up on its next occurrence instead of keeping a
    /// value that was never there.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_failure_cluster(
        &self,
        project_id: &str,
        signature: &str,
        hypothesis: &str,
        confidence: f64,
        failure_fingerprint: Option<&str>,
    ) -> Result<(Uuid, i64)> {
        let existing = sqlx::query(
            "SELECT id, occurrence_count FROM failure_clusters WHERE project_id = ? AND signature = ?",
        )
        .bind(project_id)
        .bind(signature)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(row) = existing {
            let id: String = row.get("id");
            let count: i64 = row.get::<i64, _>("occurrence_count") + 1;
            let reproducibility = (count as f64 / 5.0).min(1.0);
            if let Some(fp) = failure_fingerprint {
                sqlx::query(
                    "UPDATE failure_clusters
                     SET occurrence_count = ?, last_seen_at = ?, reproducibility = ?,
                         failure_fingerprint = COALESCE(failure_fingerprint, ?)
                     WHERE id = ?",
                )
                .bind(count)
                .bind(now_rfc3339())
                .bind(reproducibility)
                .bind(fp)
                .bind(&id)
                .execute(&self.pool)
                .await?;
            } else {
                sqlx::query(
                    "UPDATE failure_clusters SET occurrence_count = ?, last_seen_at = ?, reproducibility = ? WHERE id = ?",
                )
                .bind(count)
                .bind(now_rfc3339())
                .bind(reproducibility)
                .bind(&id)
                .execute(&self.pool)
                .await?;
            }
            return Ok((Uuid::parse_str(&id).unwrap_or_default(), count));
        }
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO failure_clusters (id, project_id, signature, first_seen_at, last_seen_at, occurrence_count, root_cause_hypothesis, confidence, reproducibility, review_state, failure_fingerprint)
             VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, 'NEEDS_REVIEW', ?)",
        )
        .bind(id.to_string())
        .bind(project_id)
        .bind(signature)
        .bind(now_rfc3339())
        .bind(now_rfc3339())
        .bind(hypothesis)
        .bind(confidence)
        .bind(0.2_f64)
        .bind(failure_fingerprint)
        .execute(&self.pool)
        .await?;
        Ok((id, 1))
    }

    /// Bounded by `LIST_FAILURE_CLUSTERS_MAX` (see the read caps at the top of
    /// this file): one row per distinct failure signature, but a long-lived
    /// project keeps minting new signatures forever.
    pub async fn list_failure_clusters(
        &self,
        project_filter: Option<&str>,
    ) -> Result<Vec<FailureClusterRow>> {
        let pid = project_filter.unwrap_or("default");
        let rows = sqlx::query(
            "SELECT id, signature, first_seen_at, last_seen_at, occurrence_count, failure_fingerprint, root_cause_hypothesis, confidence, reproducibility, review_state,
                    ai_hypothesis, ai_hypothesis_provider, ai_hypothesis_model, ai_hypothesis_generated_at
             FROM failure_clusters WHERE project_id = ? ORDER BY occurrence_count DESC LIMIT ?",
        )
        .bind(pid)
        .bind(LIST_FAILURE_CLUSTERS_MAX)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| FailureClusterRow {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                signature: r.get("signature"),
                first_seen_at: parse_ts(&r.get::<String, _>("first_seen_at")),
                last_seen_at: parse_ts(&r.get::<String, _>("last_seen_at")),
                occurrence_count: r.get("occurrence_count"),
                failure_fingerprint: r.get::<Option<String>, _>("failure_fingerprint"),
                root_cause_hypothesis: r.get("root_cause_hypothesis"),
                confidence: r.get("confidence"),
                reproducibility: r.get("reproducibility"),
                review_state: r.get("review_state"),
                ai_hypothesis: r.get("ai_hypothesis"),
                ai_hypothesis_provider: r.get("ai_hypothesis_provider"),
                ai_hypothesis_model: r.get("ai_hypothesis_model"),
                ai_hypothesis_generated_at: parse_ts_opt(r.get("ai_hypothesis_generated_at")),
            })
            .collect())
    }

    /// BE-15 (§16.2/§16.3 — "Finding" detail/analysis). A finding's
    /// underlying failure cluster (the real, existing root-cause
    /// hypothesis/reproducibility/occurrence data), fetched by the
    /// cluster's own id.
    pub async fn get_failure_cluster_by_id(&self, id: Uuid) -> Result<Option<FailureClusterRow>> {
        let row = sqlx::query(
            "SELECT id, signature, first_seen_at, last_seen_at, occurrence_count, failure_fingerprint, root_cause_hypothesis, confidence, reproducibility, review_state,
                    ai_hypothesis, ai_hypothesis_provider, ai_hypothesis_model, ai_hypothesis_generated_at
             FROM failure_clusters WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| FailureClusterRow {
            id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
            signature: r.get("signature"),
            first_seen_at: parse_ts(&r.get::<String, _>("first_seen_at")),
            last_seen_at: parse_ts(&r.get::<String, _>("last_seen_at")),
            occurrence_count: r.get("occurrence_count"),
            failure_fingerprint: r.get::<Option<String>, _>("failure_fingerprint"),
            root_cause_hypothesis: r.get("root_cause_hypothesis"),
            confidence: r.get("confidence"),
            reproducibility: r.get("reproducibility"),
            review_state: r.get("review_state"),
            ai_hypothesis: r.get("ai_hypothesis"),
            ai_hypothesis_provider: r.get("ai_hypothesis_provider"),
            ai_hypothesis_model: r.get("ai_hypothesis_model"),
            ai_hypothesis_generated_at: parse_ts_opt(r.get("ai_hypothesis_generated_at")),
        }))
    }

    pub async fn get_failure_cluster_by_id_scoped(
        &self,
        project_id: &str,
        id: Uuid,
    ) -> Result<Option<FailureClusterRow>> {
        let row = sqlx::query(
            "SELECT id, signature, first_seen_at, last_seen_at, occurrence_count, failure_fingerprint, root_cause_hypothesis, confidence, reproducibility, review_state,
                    ai_hypothesis, ai_hypothesis_provider, ai_hypothesis_model, ai_hypothesis_generated_at
             FROM failure_clusters WHERE id = ? AND project_id = ?",
        )
        .bind(id.to_string())
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| FailureClusterRow {
            id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
            signature: r.get("signature"),
            first_seen_at: parse_ts(&r.get::<String, _>("first_seen_at")),
            last_seen_at: parse_ts(&r.get::<String, _>("last_seen_at")),
            occurrence_count: r.get("occurrence_count"),
            failure_fingerprint: r.get::<Option<String>, _>("failure_fingerprint"),
            root_cause_hypothesis: r.get("root_cause_hypothesis"),
            confidence: r.get("confidence"),
            reproducibility: r.get("reproducibility"),
            review_state: r.get("review_state"),
            ai_hypothesis: r.get("ai_hypothesis"),
            ai_hypothesis_provider: r.get("ai_hypothesis_provider"),
            ai_hypothesis_model: r.get("ai_hypothesis_model"),
            ai_hypothesis_generated_at: parse_ts_opt(r.get("ai_hypothesis_generated_at")),
        }))
    }

    /// Build order item 4 — persists an on-demand AI-assisted hypothesis
    /// onto an existing failure cluster. Additive only: never touches
    /// `root_cause_hypothesis`/`confidence`/`occurrence_count`/any
    /// deterministic column `intelligence.rs::classify_failure`/
    /// `record_failure` already wrote. Only ever called on a genuine AI
    /// success (`intelligence::generate_ai_hypothesis`'s `Ai` outcome) — a
    /// failed/unavailable attempt never calls this, so the four AI columns
    /// stay exactly as they were (`NULL` if never generated before) rather
    /// than being overwritten with a fabricated value. See ACC-018's
    /// Failure Behavior criteria ("Never fabricated output").
    pub async fn set_ai_hypothesis(
        &self,
        cluster_id: Uuid,
        hypothesis: &str,
        provider: &str,
        model: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE failure_clusters SET ai_hypothesis = ?, ai_hypothesis_provider = ?, ai_hypothesis_model = ?, ai_hypothesis_generated_at = ? WHERE id = ?",
        )
        .bind(hypothesis)
        .bind(provider)
        .bind(model)
        .bind(now_rfc3339())
        .bind(cluster_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn has_bug_draft_for_cluster(&self, cluster_id: Uuid) -> Result<bool> {
        let row = sqlx::query("SELECT COUNT(*) as c FROM bug_drafts WHERE failure_cluster_id = ?")
            .bind(cluster_id.to_string())
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("c") > 0)
    }

    pub async fn insert_bug_draft(
        &self,
        failure_cluster_id: Uuid,
        title: &str,
        description: &str,
        severity: &str,
        confidence: f64,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO bug_drafts (id, failure_cluster_id, title, description, severity, confidence, review_state, created_at)
             VALUES (?, ?, ?, ?, ?, ?, 'NEEDS_REVIEW', ?)",
        )
        .bind(id.to_string())
        .bind(failure_cluster_id.to_string())
        .bind(title)
        .bind(description)
        .bind(severity)
        .bind(confidence)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// The most rows a findings read returns.
    ///
    /// `bug_drafts` is append-only and `list_bug_drafts` sits behind a polled
    /// endpoint, so an unbounded read there grows with every finding the project
    /// ever produced. The findings view reads the newest page; this bounds it to
    /// a size a UI can actually render. Same shape of clamp `list_audit`'s own
    /// `limit` already applies at its handler.
    const BUG_DRAFTS_PAGE_MAX: i64 = 300;

    /// B14 — bug drafts are project-owned, via the failure cluster they were
    /// drafted from. This previously had no project filter at all, so a
    /// project-scoped viewer key received every tenant's drafts (title,
    /// description, severity and confidence included). `bug_drafts` has no
    /// project_id column, so ownership is resolved through
    /// `failure_clusters`.
    ///
    /// `Some(pid)` restricts to that tenant (untenanted clusters counting as
    /// `"default"`); `None` is an unscoped/global caller and sees everything.
    pub async fn list_bug_drafts(&self, project_filter: Option<&str>) -> Result<Vec<BugDraftRow>> {
        // Two literal statements rather than one built by `format!`: sqlx 0.9
        // rejects runtime-constructed SQL at compile time.
        //
        // The project predicate was `COALESCE(c.project_id, 'default') = ?`,
        // which is non-sargable: the column is wrapped in a function, so no
        // index on `failure_clusters.project_id` can be used and the table is
        // scanned and grouped to answer one project's page. Rewritten as the
        // explicit disjunction that means exactly the same thing -- this project,
        // or the untenanted bucket when the caller asked for it -- leaving both
        // sides as plain column comparisons.
        //
        // `LIMIT` added because this is a polled endpoint over an append-only
        // table with no bound, so its cost grew with every finding the project
        // ever produced and the response grew with it.
        let rows = match project_filter {
            Some(pid) => {
                sqlx::query(
                    "SELECT d.id as id, d.failure_cluster_id as failure_cluster_id, d.title as title,
                            d.description as description, d.severity as severity, d.confidence as confidence,
                            d.review_state as review_state, d.created_at as created_at,
                            d.promoted_at as promoted_at, d.promoted_by as promoted_by
                     FROM bug_drafts d
                     JOIN failure_clusters c ON c.id = d.failure_cluster_id
                     WHERE c.project_id = ?1
                        OR (c.project_id IS NULL AND ?1 = 'default')
                     ORDER BY d.created_at DESC
                     LIMIT ?2",
                )
                .bind(pid)
                .bind(Self::BUG_DRAFTS_PAGE_MAX)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT d.id as id, d.failure_cluster_id as failure_cluster_id, d.title as title,
                            d.description as description, d.severity as severity, d.confidence as confidence,
                            d.review_state as review_state, d.created_at as created_at,
                            d.promoted_at as promoted_at, d.promoted_by as promoted_by
                     FROM bug_drafts d
                     ORDER BY d.created_at DESC
                     LIMIT ?1",
                )
                .bind(Self::BUG_DRAFTS_PAGE_MAX)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows
            .into_iter()
            .map(|r| BugDraftRow {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                failure_cluster_id: Uuid::parse_str(&r.get::<String, _>("failure_cluster_id"))
                    .unwrap_or_default(),
                title: r.get("title"),
                description: r.get("description"),
                severity: r.get("severity"),
                confidence: r.get("confidence"),
                review_state: r.get("review_state"),
                created_at: parse_ts(&r.get::<String, _>("created_at")),
                promoted_at: parse_ts_opt(r.get("promoted_at")),
                promoted_by: r.get("promoted_by"),
            })
            .collect())
    }

    /// The human-in-the-loop step the original PRD critique flagged as
    /// missing entirely ("Anomaly -> confirmed-defect promotion path (PRD
    /// §14) is undefined"): a NEEDS_REVIEW draft only becomes CONFIRMED or
    /// DISMISSED via this explicit, audited call — never automatically.
    /// B14 — resolves the project that owns a draft so `/bug-drafts/:id/review`
    /// can reject a cross-project mutation *before* it happens. Returns
    /// `None` when the draft does not exist. An untenanted cluster resolves
    /// to `"default"`, matching `intelligence::tenant_key`.
    /// BE-15 (§16.2 Finding Detail) — a single finding by id.
    pub async fn get_bug_draft_by_id(&self, id: Uuid) -> Result<Option<BugDraftRow>> {
        let row = sqlx::query(
            "SELECT id, failure_cluster_id, title, description, severity, confidence, review_state, created_at, promoted_at, promoted_by
             FROM bug_drafts WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| BugDraftRow {
            id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
            failure_cluster_id: Uuid::parse_str(&r.get::<String, _>("failure_cluster_id"))
                .unwrap_or_default(),
            title: r.get("title"),
            description: r.get("description"),
            severity: r.get("severity"),
            confidence: r.get("confidence"),
            review_state: r.get("review_state"),
            created_at: parse_ts(&r.get::<String, _>("created_at")),
            promoted_at: parse_ts_opt(r.get("promoted_at")),
            promoted_by: r.get("promoted_by"),
        }))
    }

    pub async fn bug_draft_project(&self, id: Uuid) -> Result<Option<String>> {
        let row = sqlx::query(
            "SELECT COALESCE(c.project_id, 'default') as project_id
             FROM bug_drafts d
             JOIN failure_clusters c ON c.id = d.failure_cluster_id
             WHERE d.id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<String, _>("project_id")))
    }

    pub async fn review_bug_draft(
        &self,
        id: Uuid,
        new_state: &str,
        reviewed_by: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE bug_drafts SET review_state = ?, promoted_at = ?, promoted_by = ? WHERE id = ?",
        )
        .bind(new_state)
        .bind(now_rfc3339())
        .bind(reviewed_by)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ------------------------------------------------------------- audit

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_audit(
        &self,
        actor: Option<&str>,
        role: Option<&str>,
        project_id: Option<&str>,
        action: &str,
        target: Option<&str>,
        detail: Option<&str>,
        outcome: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit_log (id, occurred_at, actor, role, project_id, action, target, detail, outcome)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(now_rfc3339())
        .bind(actor)
        .bind(role)
        .bind(project_id)
        .bind(action)
        .bind(target)
        .bind(detail)
        .bind(outcome)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// BE-15 (§25.1) — `project_filter: Some(pid)` restricts to that
    /// project's own audit rows (`project_id = ?`, no "default" sentinel:
    /// unlike `quality_nodes`, a `NULL` audit row genuinely means "no
    /// project context" — an admin action — and correctly never matches a
    /// real project's scoped view). `None` is the legacy unscoped
    /// behavior (`/admin/audit-log`), unchanged.
    ///
    /// `offset` (added alongside real pagination — previously this was
    /// always 0, "limit" being the sprint's entire pagination story per
    /// `AuditPage.tsx`'s own honest doc comment on that gap): plain
    /// `OFFSET`, not a keyset/cursor scheme. `AuditRow` has no real unique
    /// id or monotonic sequence column to key a cursor off (re-confirmed
    /// here, not assumed) — `occurred_at` alone is not guaranteed distinct
    /// under concurrent writes — and this table's real size (low
    /// thousands of rows for a single project, not millions) does not
    /// justify the added complexity a real keyset scheme would need to be
    /// done correctly. Paired with `count_audit` below so a caller can
    /// show a real "page N of M" / "X-Y of Z" rather than guessing from a
    /// fetched batch's length.
    /// Adds one answered AI call to the running totals of its provider, model and task (`ai_call_stats`, migration 0021).
    /// The tokens count only when the provider reported both, so `token_calls` is how many calls they are an average of.
    pub async fn record_ai_call(
        &self,
        provider: &str,
        model: &str,
        task: &str,
        latency_ms: u64,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
    ) -> Result<()> {
        let (token_calls, prompt, completion) = match (prompt_tokens, completion_tokens) {
            (Some(p), Some(c)) => (1i64, i64::from(p), i64::from(c)),
            _ => (0, 0, 0),
        };
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO ai_call_stats
                 (provider, model, task, calls, total_ms, token_calls, prompt_tokens, completion_tokens, first_at, last_at)
             VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (provider, model, task) DO UPDATE SET
                 calls = calls + 1,
                 total_ms = total_ms + excluded.total_ms,
                 token_calls = token_calls + excluded.token_calls,
                 prompt_tokens = prompt_tokens + excluded.prompt_tokens,
                 completion_tokens = completion_tokens + excluded.completion_tokens,
                 last_at = excluded.last_at",
        )
        .bind(provider)
        .bind(model)
        .bind(task)
        .bind(i64::try_from(latency_ms).unwrap_or(i64::MAX))
        .bind(token_calls)
        .bind(prompt)
        .bind(completion)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// What the calls of `task` took so far, one row per provider and model (`record_ai_call`).
    pub async fn ai_call_stats(&self, task: &str) -> Result<Vec<AiCallStatRow>> {
        let rows = sqlx::query(
            "SELECT provider, model, task, calls, total_ms, token_calls, prompt_tokens, completion_tokens
             FROM ai_call_stats WHERE task = ? ORDER BY provider, model",
        )
        .bind(task)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| AiCallStatRow {
                provider: r.get("provider"),
                model: r.get("model"),
                task: r.get("task"),
                calls: r.get("calls"),
                total_ms: r.get("total_ms"),
                token_calls: r.get("token_calls"),
                prompt_tokens: r.get("prompt_tokens"),
                completion_tokens: r.get("completion_tokens"),
            })
            .collect())
    }

    pub async fn list_audit(
        &self,
        project_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AuditRow>> {
        let rows =
            match project_filter {
                Some(pid) => sqlx::query(
                    "SELECT occurred_at, actor, role, project_id, action, target, detail, outcome
                     FROM audit_log WHERE project_id = ? ORDER BY occurred_at DESC LIMIT ? OFFSET ?",
                )
                .bind(pid)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?,
                None => sqlx::query(
                    "SELECT occurred_at, actor, role, project_id, action, target, detail, outcome
                     FROM audit_log ORDER BY occurred_at DESC LIMIT ? OFFSET ?",
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?,
            };
        Ok(rows
            .into_iter()
            .map(|r| AuditRow {
                occurred_at: parse_ts(&r.get::<String, _>("occurred_at")),
                actor: r.get("actor"),
                role: r.get("role"),
                project_id: r.get("project_id"),
                action: r.get("action"),
                target: r.get("target"),
                detail: r.get("detail"),
                outcome: r.get("outcome"),
            })
            .collect())
    }

    /// The real total row count `list_audit`'s own `limit`/`offset` window
    /// is a slice of — same `project_filter` semantics as `list_audit`
    /// above. A real `COUNT(*)`, not a batch-length guess, so a caller can
    /// show an honest "X-Y of Z" / disable "Next" exactly at the real end,
    /// not one batch-length short or long.
    pub async fn count_audit(&self, project_filter: Option<&str>) -> Result<i64> {
        let count: i64 = match project_filter {
            Some(pid) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE project_id = ?")
                    .bind(pid)
                    .fetch_one(&self.pool)
                    .await?
            }
            None => {
                sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
                    .fetch_one(&self.pool)
                    .await?
            }
        };
        Ok(count)
    }

    // ---------------------------------------------------------------- healing

    pub async fn insert_healing_proposal(&self, p: &HealingProposalRow) -> Result<()> {
        let req_appr = if p.requires_approval { 1i64 } else { 0i64 };
        let app_at_s = p.approved_at.map(|t| t.to_rfc3339());
        sqlx::query(
            r#"INSERT INTO healing_proposals (
                id, project_id, run_id, task_id, failure_fingerprint, diagnosis_category,
                confidence, risk_level, requires_approval, original_actions, proposed_actions,
                ast_diff, rationale, evidence_refs, state, idempotency_key, healing_run_id,
                verification_result, approved_by, approved_at, rejection_reason, created_at,
                expires_at, updated_at, context
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(p.id.to_string())
        .bind(&p.project_id)
        .bind(p.run_id.to_string())
        .bind(&p.task_id)
        .bind(&p.failure_fingerprint)
        .bind(&p.diagnosis_category)
        .bind(p.confidence)
        .bind(&p.risk_level)
        .bind(req_appr)
        .bind(&p.original_actions)
        .bind(&p.proposed_actions)
        .bind(&p.ast_diff)
        .bind(&p.rationale)
        .bind(&p.evidence_refs)
        .bind(&p.state)
        .bind(&p.idempotency_key)
        .bind(p.healing_run_id.map(|u| u.to_string()))
        .bind(&p.verification_result)
        .bind(&p.approved_by)
        .bind(app_at_s)
        .bind(&p.rejection_reason)
        .bind(p.created_at.to_rfc3339())
        .bind(p.expires_at.to_rfc3339())
        .bind(p.updated_at.to_rfc3339())
        .bind(&p.context)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_healing_proposal(&self, id: Uuid) -> Result<Option<HealingProposalRow>> {
        let row = sqlx::query("SELECT * FROM healing_proposals WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(healing_proposal_from_row).transpose()
    }

    pub async fn get_healing_proposal_by_idempotency(
        &self,
        project_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<HealingProposalRow>> {
        let row = sqlx::query(
            "SELECT * FROM healing_proposals WHERE project_id = ? AND idempotency_key = ?",
        )
        .bind(project_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await?;
        row.map(healing_proposal_from_row).transpose()
    }

    /// The proposals a project has raised, newest first. Bounded by
    /// `LIST_HEALING_PROPOSALS_MAX`: `healing_proposals` is append-only and
    /// gains a row for every failed task, so an unbounded read grew with every
    /// failure the project ever had. Note `list_healing_proposals_for_cluster`
    /// reads through this, so it now correlates against the newest page too.
    pub async fn list_healing_proposals(
        &self,
        project_id: &str,
        state: Option<&str>,
    ) -> Result<Vec<HealingProposalRow>> {
        let rows = if let Some(s) = state {
            sqlx::query(
                "SELECT * FROM healing_proposals WHERE project_id = ? AND state = ? ORDER BY created_at DESC LIMIT ?",
            )
            .bind(project_id)
            .bind(s)
            .bind(LIST_HEALING_PROPOSALS_MAX)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT * FROM healing_proposals WHERE project_id = ? ORDER BY created_at DESC LIMIT ?",
            )
            .bind(project_id)
            .bind(LIST_HEALING_PROPOSALS_MAX)
            .fetch_all(&self.pool)
            .await?
        };
        rows.into_iter().map(healing_proposal_from_row).collect()
    }

    pub async fn count_healing_attempts(
        &self,
        project_id: &str,
        run_id: &str,
        task_id: &str,
    ) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM healing_proposals WHERE project_id = ? AND run_id = ? AND task_id = ?",
        )
        .bind(project_id)
        .bind(run_id)
        .bind(task_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// True while some proposal for this task is still live: waiting on a human
    /// (`PENDING_APPROVAL`) or approved but not yet run, and not past its `expires_at`.
    /// `propose_healing` bounds attempts per *run*, so a task that keeps failing across runs
    /// would otherwise collect one new proposal per run; the autonomous path uses this to
    /// raise one and wait. Expiry is checked here, not only in `approve`, because a proposal
    /// nobody ever opens is never flipped to `EXPIRED` and would otherwise block the task forever.
    /// Same notion of "live" as the retention shield in `prune_execution_telemetry`.
    pub async fn has_open_healing_proposal(
        &self,
        project_id: &str,
        task_id: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM healing_proposals
               WHERE project_id = ? AND task_id = ?
                 AND state IN ('PENDING_APPROVAL', 'APPROVED')
                 AND expires_at > ?"#,
        )
        .bind(project_id)
        .bind(task_id)
        .bind(now.to_rfc3339())
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    pub async fn update_healing_proposal_state(
        &self,
        id: Uuid,
        state: &str,
        approved_by: Option<&str>,
        approved_at: Option<chrono::DateTime<chrono::Utc>>,
        rejection_reason: Option<&str>,
        healing_run_id: Option<Uuid>,
        verification_result: Option<&str>,
    ) -> Result<()> {
        let now = now_rfc3339();
        let app_at_s = approved_at.map(|t| t.to_rfc3339());
        let heal_run_s = healing_run_id.map(|u| u.to_string());
        sqlx::query(
            r#"UPDATE healing_proposals SET
                state = ?,
                approved_by = COALESCE(?, approved_by),
                approved_at = COALESCE(?, approved_at),
                rejection_reason = COALESCE(?, rejection_reason),
                healing_run_id = COALESCE(?, healing_run_id),
                verification_result = COALESCE(?, verification_result),
                updated_at = ?
            WHERE id = ?"#,
        )
        .bind(state)
        .bind(approved_by)
        .bind(app_at_s)
        .bind(rejection_reason)
        .bind(heal_run_s)
        .bind(verification_result)
        .bind(now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_healing_verification_metric(
        &self,
        project_id: &str,
        strategy: &str,
        failure_category: &str,
        outcome: &str,
    ) -> Result<HealingStrategyMetricsRow> {
        let existing = sqlx::query(
            "SELECT * FROM healing_strategy_metrics WHERE project_id = ? AND strategy = ? AND failure_category = ?",
        )
        .bind(project_id)
        .bind(strategy)
        .bind(failure_category)
        .fetch_optional(&self.pool)
        .await?;

        let now = chrono::Utc::now();
        let is_verified = outcome == "VERIFIED";
        let is_not_verified = outcome == "NOT_VERIFIED";
        let is_regression = outcome == "REGRESSION_INTRODUCED";

        if let Some(r) = existing {
            let mut row = healing_strategy_metrics_from_row(r)?;
            row.attempts_count += 1;
            if is_verified {
                row.verified_count += 1;
            }
            if is_not_verified {
                row.not_verified_count += 1;
            }
            if is_regression {
                row.regression_count += 1;
            }
            let base = if row.attempts_count > 0 {
                row.verified_count as f64 / row.attempts_count as f64
            } else {
                0.5
            };
            let penalty = if row.attempts_count > 0 {
                (row.regression_count as f64 / row.attempts_count as f64) * 0.5
            } else {
                0.0
            };
            row.calibrated_confidence = (base - penalty).clamp(0.05, 0.99);
            row.updated_at = now;

            sqlx::query(
                r#"UPDATE healing_strategy_metrics SET
                    attempts_count = ?,
                    verified_count = ?,
                    not_verified_count = ?,
                    regression_count = ?,
                    calibrated_confidence = ?,
                    updated_at = ?
                WHERE id = ?"#,
            )
            .bind(row.attempts_count)
            .bind(row.verified_count)
            .bind(row.not_verified_count)
            .bind(row.regression_count)
            .bind(row.calibrated_confidence)
            .bind(row.updated_at.to_rfc3339())
            .bind(row.id.to_string())
            .execute(&self.pool)
            .await?;

            Ok(row)
        } else {
            let id = Uuid::new_v4();
            let attempts_count = 1i64;
            let verified_count = if is_verified { 1i64 } else { 0i64 };
            let not_verified_count = if is_not_verified { 1i64 } else { 0i64 };
            let regression_count = if is_regression { 1i64 } else { 0i64 };
            let base: f64 = if is_verified { 0.90 } else { 0.30 };
            let penalty: f64 = if is_regression { 0.25 } else { 0.0 };
            let calibrated_confidence = (base - penalty).clamp(0.05, 0.99);

            let row = HealingStrategyMetricsRow {
                id,
                project_id: project_id.to_string(),
                strategy: strategy.to_string(),
                failure_category: failure_category.to_string(),
                attempts_count,
                verified_count,
                not_verified_count,
                regression_count,
                calibrated_confidence,
                updated_at: now,
            };

            sqlx::query(
                r#"INSERT INTO healing_strategy_metrics (
                    id, project_id, strategy, failure_category, attempts_count,
                    verified_count, not_verified_count, regression_count, calibrated_confidence,
                    updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(row.id.to_string())
            .bind(&row.project_id)
            .bind(&row.strategy)
            .bind(&row.failure_category)
            .bind(row.attempts_count)
            .bind(row.verified_count)
            .bind(row.not_verified_count)
            .bind(row.regression_count)
            .bind(row.calibrated_confidence)
            .bind(row.updated_at.to_rfc3339())
            .execute(&self.pool)
            .await?;

            Ok(row)
        }
    }

    pub async fn get_healing_strategy_metrics(
        &self,
        project_id: &str,
        strategy: &str,
        failure_category: &str,
    ) -> Result<Option<HealingStrategyMetricsRow>> {
        let row = sqlx::query(
            "SELECT * FROM healing_strategy_metrics WHERE project_id = ? AND strategy = ? AND failure_category = ?",
        )
        .bind(project_id)
        .bind(strategy)
        .bind(failure_category)
        .fetch_optional(&self.pool)
        .await?;
        row.map(healing_strategy_metrics_from_row).transpose()
    }

    pub async fn list_healing_strategy_metrics(
        &self,
        project_id: &str,
    ) -> Result<Vec<HealingStrategyMetricsRow>> {
        let rows = sqlx::query(
            "SELECT * FROM healing_strategy_metrics WHERE project_id = ? ORDER BY strategy ASC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(healing_strategy_metrics_from_row)
            .collect()
    }

    // ------------------------------------------------------------- execution_telemetry (Phase 6)

    pub async fn insert_execution_telemetry(
        &self,
        t: &crate::adaptive_intelligence::ExecutionTelemetry,
    ) -> Result<()> {
        let evidence_json =
            serde_json::to_string(&t.evidence_refs).unwrap_or_else(|_| "[]".to_string());
        sqlx::query(
            r#"INSERT INTO execution_telemetry (
                id, project_id, run_id, task_id, task_name, test_case_id, execution_id,
                executor, environment, start_time, end_time, duration_ms, status,
                retry_count, healing_attempt_count, failure_category, failure_fingerprint,
                error_message, evidence_refs, assertion_result, http_status, selector,
                verification_outcome, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(t.id.to_string())
        .bind(&t.project_id)
        .bind(t.run_id.to_string())
        .bind(&t.task_id)
        .bind(&t.task_name)
        .bind(t.test_case_id.map(|u| u.to_string()))
        .bind(&t.execution_id)
        .bind(&t.executor)
        .bind(&t.environment)
        .bind(t.start_time.to_rfc3339())
        .bind(t.end_time.map(|ts| ts.to_rfc3339()))
        .bind(t.duration_ms as i64)
        .bind(t.status.as_str())
        .bind(t.retry_count as i64)
        .bind(t.healing_attempt_count as i64)
        .bind(t.failure_category.map(|c| c.as_str().to_string()))
        .bind(&t.failure_fingerprint)
        .bind(&t.error_message)
        .bind(evidence_json)
        .bind(&t.assertion_result)
        .bind(t.http_status.map(|s| s as i64))
        .bind(&t.selector)
        .bind(&t.verification_outcome)
        .bind(t.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_execution_telemetry(
        &self,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<ExecutionTelemetryRow>> {
        let rows = sqlx::query(
            "SELECT * FROM execution_telemetry WHERE project_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(execution_telemetry_from_row).collect()
    }

    pub async fn count_execution_telemetry(&self, project_id: Option<&str>) -> Result<i64> {
        let count: i64 = match project_id {
            Some(pid) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM execution_telemetry WHERE project_id = ?")
                    .bind(pid)
                    .fetch_one(&self.pool)
                    .await?
            }
            None => {
                sqlx::query_scalar("SELECT COUNT(*) FROM execution_telemetry")
                    .fetch_one(&self.pool)
                    .await?
            }
        };
        Ok(count)
    }

    pub async fn list_flakiness_metrics(
        &self,
        project_id: &str,
    ) -> Result<Vec<FlakinessMetricsRow>> {
        let rows = sqlx::query(
            "SELECT fm.* FROM flakiness_metrics fm JOIN quality_nodes qn ON fm.node_id = qn.id WHERE qn.project_id = ?",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::new();
        for r in rows {
            out.push(FlakinessMetricsRow {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                node_id: Uuid::parse_str(&r.get::<String, _>("node_id")).unwrap_or_default(),
                flakiness_score: r.get("flakiness_score"),
                window_runs: r.get("window_runs"),
                window_failures: r.get("window_failures"),
                environment_correlation: r.get("environment_correlation"),
                computed_at: chrono::DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("computed_at"),
                )
                .unwrap_or_default()
                .with_timezone(&chrono::Utc),
            });
        }
        Ok(out)
    }

    pub async fn list_quality_nodes(&self, project_id: &str) -> Result<Vec<QualityNodeRow>> {
        let rows = sqlx::query(
            "SELECT * FROM quality_nodes WHERE project_id = ? ORDER BY created_at, id LIMIT ?",
        )
        .bind(project_id)
        .bind(LIST_QUALITY_NODES_MAX)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for r in rows {
            out.push(QualityNodeRow {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                project_id: r.get("project_id"),
                node_type: r.get("node_type"),
                external_ref: r.get("external_ref"),
                name: r.get("name"),
                metadata: r.get("metadata"),
                created_at: chrono::DateTime::parse_from_rfc3339(&r.get::<String, _>("created_at"))
                    .unwrap_or_default()
                    .with_timezone(&chrono::Utc),
            });
        }
        Ok(out)
    }

    pub async fn list_quality_relations_for_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<QualityRelationRow>> {
        let rows = sqlx::query(
            "SELECT qr.* FROM quality_relations qr JOIN quality_nodes qn ON qr.from_node_id = qn.id
             WHERE qn.project_id = ? ORDER BY qr.created_at, qr.id LIMIT ?",
        )
        .bind(project_id)
        .bind(LIST_QUALITY_RELATIONS_MAX)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for r in rows {
            out.push(QualityRelationRow {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                from_node_id: Uuid::parse_str(&r.get::<String, _>("from_node_id"))
                    .unwrap_or_default(),
                to_node_id: Uuid::parse_str(&r.get::<String, _>("to_node_id")).unwrap_or_default(),
                relation_type: r.get("relation_type"),
                metadata: r.get("metadata"),
                created_at: chrono::DateTime::parse_from_rfc3339(&r.get::<String, _>("created_at"))
                    .unwrap_or_default()
                    .with_timezone(&chrono::Utc),
            });
        }
        Ok(out)
    }

    pub async fn get_task_telemetry_history(
        &self,
        project_id: &str,
        task_name: &str,
        environment: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ExecutionTelemetryRow>> {
        let rows = match environment {
            Some(env) => {
                sqlx::query(
                    "SELECT * FROM execution_telemetry WHERE project_id = ? AND task_name = ? AND environment = ? ORDER BY created_at ASC LIMIT ?",
                )
                .bind(project_id)
                .bind(task_name)
                .bind(env)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT * FROM execution_telemetry WHERE project_id = ? AND task_name = ? ORDER BY created_at ASC LIMIT ?",
                )
                .bind(project_id)
                .bind(task_name)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };

        rows.into_iter().map(execution_telemetry_from_row).collect()
    }

    // ------------------------------------------------------------- operations & cross-env correlation (Phase 8)

    pub async fn get_failure_cluster_by_signature(
        &self,
        project_id: &str,
        signature: &str,
    ) -> Result<Option<FailureClusterRow>> {
        let row = sqlx::query(
            "SELECT id, signature, first_seen_at, last_seen_at, occurrence_count, failure_fingerprint, root_cause_hypothesis, confidence, reproducibility, review_state,
                    ai_hypothesis, ai_hypothesis_provider, ai_hypothesis_model, ai_hypothesis_generated_at
             FROM failure_clusters WHERE project_id = ? AND signature = ?",
        )
        .bind(project_id)
        .bind(signature)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| FailureClusterRow {
            id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
            signature: r.get("signature"),
            first_seen_at: parse_ts(&r.get::<String, _>("first_seen_at")),
            last_seen_at: parse_ts(&r.get::<String, _>("last_seen_at")),
            occurrence_count: r.get("occurrence_count"),
            failure_fingerprint: r.get::<Option<String>, _>("failure_fingerprint"),
            root_cause_hypothesis: r.get("root_cause_hypothesis"),
            confidence: r.get("confidence"),
            reproducibility: r.get("reproducibility"),
            review_state: r.get("review_state"),
            ai_hypothesis: r.get("ai_hypothesis"),
            ai_hypothesis_provider: r.get("ai_hypothesis_provider"),
            ai_hypothesis_model: r.get("ai_hypothesis_model"),
            ai_hypothesis_generated_at: parse_ts_opt(r.get("ai_hypothesis_generated_at")),
        }))
    }

    pub async fn list_telemetry_for_cluster(
        &self,
        project_id: &str,
        signature: &str,
        fingerprint: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ExecutionTelemetryRow>> {
        let rows = if let Some(fp) = fingerprint {
            sqlx::query(
                "SELECT * FROM execution_telemetry
                 WHERE project_id = ?
                   AND (failure_fingerprint = ? OR failure_fingerprint = ? OR error_message LIKE ? OR task_name = ?)
                 ORDER BY created_at ASC LIMIT ?",
            )
            .bind(project_id)
            .bind(signature)
            .bind(fp)
            .bind(format!("%{signature}%"))
            .bind(signature)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT * FROM execution_telemetry
                 WHERE project_id = ?
                   AND (failure_fingerprint = ? OR error_message LIKE ? OR task_name = ?)
                 ORDER BY created_at ASC LIMIT ?",
            )
            .bind(project_id)
            .bind(signature)
            .bind(format!("%{signature}%"))
            .bind(signature)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        rows.into_iter().map(execution_telemetry_from_row).collect()
    }

    pub async fn list_healing_proposals_for_cluster(
        &self,
        project_id: &str,
        fingerprint: Option<&str>,
        task_ids: &[String],
    ) -> Result<Vec<HealingProposalRow>> {
        let all = self.list_healing_proposals(project_id, None).await?;
        let filtered = all
            .into_iter()
            .filter(|p| {
                if let Some(fp) = fingerprint {
                    if p.failure_fingerprint == fp {
                        return true;
                    }
                }
                task_ids.iter().any(|tid| tid == &p.task_id)
            })
            .collect();
        Ok(filtered)
    }

    pub async fn prune_execution_telemetry(
        &self,
        project_id: &str,
        older_than: chrono::DateTime<chrono::Utc>,
        dry_run: bool,
    ) -> Result<u64> {
        let older_str = older_than.to_rfc3339();
        // A proposal shields its task's telemetry only while it can still be acted on. Expiry is
        // otherwise applied lazily (only when someone approves), so an unopened proposal stays
        // `PENDING_APPROVAL` indefinitely and, without this bound, would pin that task's
        // telemetry out of retention for good.
        let now_str = chrono::Utc::now().to_rfc3339();

        let eligible_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM execution_telemetry
               WHERE project_id = ?
                 AND created_at < ?
                 AND run_id NOT IN (SELECT id FROM runs WHERE project_id = ? AND state IN ('PENDING', 'RUNNING'))
                 AND task_id NOT IN (SELECT task_id FROM healing_proposals WHERE project_id = ? AND state IN ('PENDING_APPROVAL', 'APPROVED') AND expires_at > ?)"#
        )
        .bind(project_id)
        .bind(&older_str)
        .bind(project_id)
        .bind(project_id)
        .bind(&now_str)
        .fetch_one(&self.pool)
        .await?;

        if dry_run {
            return Ok(eligible_count.max(0) as u64);
        }

        let res = sqlx::query(
            r#"DELETE FROM execution_telemetry
               WHERE project_id = ?
                 AND created_at < ?
                 AND run_id NOT IN (SELECT id FROM runs WHERE project_id = ? AND state IN ('PENDING', 'RUNNING'))
                 AND task_id NOT IN (SELECT task_id FROM healing_proposals WHERE project_id = ? AND state IN ('PENDING_APPROVAL', 'APPROVED') AND expires_at > ?)"#
        )
        .bind(project_id)
        .bind(&older_str)
        .bind(project_id)
        .bind(project_id)
        .bind(&now_str)
        .execute(&self.pool)
        .await?;

        Ok(res.rows_affected())
    }

    /// Deletes a project's own audit rows older than `older_than`, returning how
    /// many went. `dry_run` counts without deleting, exactly as
    /// `prune_execution_telemetry` does, so an operator can see the blast radius
    /// before committing to it.
    ///
    /// `audit_log` had no retention of any kind: one row per audited action,
    /// appended forever, on a table every state-changing handler writes to. The
    /// existing retention pass deliberately did not touch it (see this crate's
    /// `operations` module: it prunes telemetry while "preserving audit logs"),
    /// which is the right default for a trail — but it leaves unbounded growth
    /// with no operator control at all. This is that control, opt-in.
    ///
    /// Two deliberate limits on what it may delete:
    ///
    /// - **Project-scoped only, never global.** `audit_log.project_id` is
    ///   nullable and rows written without a tenant (`auth.fail` on an
    ///   unauthenticated attempt, and system actions) have `NULL` there. The
    ///   predicate is an equality on a bound tenant, so those rows never match.
    ///   A project admin pruning their own project cannot destroy shared audit
    ///   history.
    /// - **Nothing but this project.** There is no unscoped variant of this
    ///   method on purpose.
    ///
    /// `occurred_at` is TEXT holding RFC3339, so the cutoff compares as a string
    /// — the same assumption `prune_execution_telemetry` already makes about
    /// `created_at`, and the same one that holds only while every writer goes
    /// through `insert_audit`.
    pub async fn prune_audit(
        &self,
        project_id: &str,
        older_than: chrono::DateTime<chrono::Utc>,
        dry_run: bool,
    ) -> Result<u64> {
        let older_str = older_than.to_rfc3339();

        if dry_run {
            let eligible: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM audit_log WHERE project_id = ? AND occurred_at < ?",
            )
            .bind(project_id)
            .bind(&older_str)
            .fetch_one(&self.pool)
            .await?;
            return Ok(eligible.max(0) as u64);
        }

        let res = sqlx::query("DELETE FROM audit_log WHERE project_id = ? AND occurred_at < ?")
            .bind(project_id)
            .bind(&older_str)
            .execute(&self.pool)
            .await?;

        Ok(res.rows_affected())
    }
}

// --------------------------------------------------------------- row DTOs

/// BE-8 — API Contract §5 Project resource. Deliberately minimal: exactly
/// the fields §5.2's "authoritative metadata" requires, nothing the
/// contract doesn't ask for (no nested run counts — that's Dashboard's job,
/// §23, not this resource's).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// A real, persisted custom skill (see migration `0009_custom_skills.sql`).
/// `id` is a plain string (not a `Uuid`) so it slots into
/// `ai_generation.rs::SkillInput.id` — the same field the frontend already
/// sends for built-in skills (`skill-functional-testing`, etc.) — without
/// either side needing a `Uuid`/`String` conversion.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CustomSkillRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_file: Option<String>,
    /// Real YAML frontmatter from an imported SKILL.md (version, author,
    /// frameworks, ...), preserved as the file wrote it. `None` for a skill
    /// created by hand.
    pub metadata: Option<serde_json::Value>,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A real, persisted custom AI provider (see migration
/// `0013_custom_ai_providers.sql`). Deliberately does NOT derive
/// `Serialize` — this row's `api_key` is real, live-usable plaintext, and
/// the lack of a derive forces every caller to explicitly build a redacted
/// view rather than accidentally returning this type directly from an HTTP
/// handler.
#[derive(Debug, Clone)]
pub struct CustomAiProviderRow {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    /// USD per 1,000 tokens in / out, as entered by the user. Both set or both `None`.
    pub input_usd_per_1k: Option<f64>,
    pub output_usd_per_1k: Option<f64>,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EvidenceRow {
    pub id: Uuid,
    pub task_id: Option<String>,
    pub kind: String,
    pub object_key: String,
    pub content_type: Option<String>,
    pub size_bytes: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct ApiKeyRow {
    pub id: Uuid,
    pub name: String,
    pub role: String,
    pub project_id: Option<String>,
}

/// AUD-005 — the persisted shape of a Test Case (`GET`/`POST .../test-cases`
/// response). Field set matches `CreateTestCaseRequest` one-to-one, plus the
/// fields only the backend ever assigns: `id`, `source`, `status`,
/// `validation_notes`, `executed_run_id`/`executed_task_id`, timestamps.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TestCaseRow {
    pub id: Uuid,
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub priority: String,
    pub severity: String,
    pub preconditions: Vec<String>,
    pub steps: Vec<TestStep>,
    pub expected_result: String,
    pub assertions: Vec<String>,
    pub test_data: Option<String>,
    pub source_document: Option<String>,
    pub requirement_reference: Option<String>,
    pub source_requirement: Option<String>,
    pub traceability: Option<serde_json::Value>,
    pub risk_factor: Option<String>,
    pub tags: Vec<String>,
    pub source: String,
    pub status: TestCaseStatus,
    pub validation_notes: Option<String>,
    pub executed_run_id: Option<Uuid>,
    pub executed_task_id: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Phase 5 (Output maturity) — `None` for every case that predates this
    /// column, and for any case deliberately left outside a suite.
    pub suite_id: Option<Uuid>,
}

/// Vector Store v1 (`0014_test_case_embeddings.sql`'s own comment). Just
/// the two fields `ai_generation::rank_similar_test_cases`'s brute-force
/// cosine-similarity scoring actually needs — `model`/`dimensions`/
/// timestamps live in the table but have no real reader yet, so they are
/// not carried into this struct (same "no field beyond what a real reader
/// needs" discipline `TestCaseRow`'s own doc comment states).
#[derive(Debug, Clone)]
pub struct TestCaseEmbeddingRow {
    pub test_case_id: Uuid,
    pub embedding: Vec<f32>,
}

/// Phase 2 (document-level RAG, `0017_documents.sql`'s own comment). Just
/// the two fields `ai_generation::rank_similar_documents`'s brute-force
/// cosine-similarity scoring actually needs — same "no field beyond what a
/// real reader needs" discipline `TestCaseEmbeddingRow` above already
/// established.
#[derive(Debug, Clone)]
pub struct DocumentEmbeddingRow {
    pub id: Uuid,
    pub embedding: Vec<f32>,
}

/// Phase 2 (document-level RAG). A project's own real, previously-extracted
/// document — `api.rs::find_similar_documents` fetches this after ranking
/// to build the real excerpt a generation prompt actually sees.
#[derive(Debug, Clone)]
pub struct DocumentRow {
    pub id: Uuid,
    pub project_id: String,
    pub file_name: String,
    pub text: String,
    pub char_count: i64,
    pub truncated: bool,
}

/// A document read for its requirements (`0020_requirements.sql`).
/// `requirements_count` is counted when the row is read, not stored.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RequirementDocumentRow {
    pub id: Uuid,
    pub file_name: String,
    pub doc_type: Option<String>,
    pub char_count: i64,
    pub requirements_count: i64,
    pub sections_count: i64,
    pub skipped_sections: Vec<String>,
    pub furniture_lines: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// One requirement read out of a document (`0020_requirements.sql`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct RequirementRow {
    pub document_id: Uuid,
    /// The id people see and a generated test case carries.
    pub key: String,
    pub ordinal: i64,
    pub section: Option<String>,
    pub kind: String,
    pub text: String,
}

/// Phase 5 (Output maturity). `suite_count` is real, computed at query
/// time (`COUNT(*)` over `test_suites.plan_id`) — never a cached/stale
/// counter that could drift from the real rows.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TestPlanRow {
    pub id: Uuid,
    pub project_id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: TestPlanStatus,
    pub entry_criteria: Option<String>,
    pub exit_criteria: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub suite_count: i64,
}

/// `case_count` is the same kind of real, computed-at-query-time count
/// `TestPlanRow::suite_count` is.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TestSuiteRow {
    pub id: Uuid,
    pub project_id: String,
    pub plan_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub case_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PerformanceRunRow {
    pub id: Uuid,
    pub project_id: String,
    pub name: String,
    pub test_type: String,
    pub status: String,
    pub verdict: Option<String>,
    pub config_json: String,
    pub metrics_json: Option<String>,
    pub verdict_reasons_json: Option<String>,
    pub error_reason: Option<String>,
    pub evidence_id: Option<Uuid>,
    pub created_by: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiKeyListRow {
    pub id: Uuid,
    pub name: String,
    pub role: String,
    pub project_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct TaskHistoryRow {
    pub status: TaskStatus,
    pub finished_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TestIntelRow {
    pub id: Uuid,
    pub name: String,
    pub risk_score: Option<f64>,
    pub failure_rate: Option<f64>,
    pub dependency_count: Option<i64>,
    pub execution_cost_ms: Option<f64>,
    pub value_score: Option<f64>,
    pub flakiness_score: Option<f64>,
    pub window_runs: Option<i64>,
    pub window_failures: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FailureClusterRow {
    pub id: Uuid,
    pub signature: String,
    pub first_seen_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub occurrence_count: i64,
    /// Phase 6 (migration 0030): the telemetry-side SHA-256 for this cluster's
    /// failure, which is the only value that joins a cluster to the executions
    /// that produced it. `None` for clusters recorded before the migration and never
    /// seen again.
    pub failure_fingerprint: Option<String>,
    pub root_cause_hypothesis: Option<String>,
    pub confidence: Option<f64>,
    pub reproducibility: Option<f64>,
    pub review_state: String,
    /// Build order item 4 — an on-demand, AI-assisted hypothesis, additive
    /// to (never a replacement for) `root_cause_hypothesis` above. `None`
    /// until `POST .../findings/:id/ai-hypothesis` is explicitly called and
    /// genuinely succeeds — see `intelligence.rs::generate_ai_hypothesis`.
    pub ai_hypothesis: Option<String>,
    pub ai_hypothesis_provider: Option<String>,
    pub ai_hypothesis_model: Option<String>,
    pub ai_hypothesis_generated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BugDraftRow {
    pub id: Uuid,
    pub failure_cluster_id: Uuid,
    pub title: String,
    pub description: String,
    pub severity: Option<String>,
    pub confidence: Option<f64>,
    pub review_state: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub promoted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub promoted_by: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditRow {
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub actor: Option<String>,
    pub role: Option<String>,
    pub project_id: Option<String>,
    pub action: String,
    pub target: Option<String>,
    pub detail: Option<String>,
    pub outcome: String,
}

/// The running totals of the answered calls of one task to one provider and model (`ai_call_stats`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AiCallStatRow {
    pub provider: String,
    pub model: String,
    pub task: String,
    pub calls: i64,
    pub total_ms: i64,
    /// How many of the calls reported token counts (the tokens below are sums over just those).
    pub token_calls: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealingProposalRow {
    pub id: Uuid,
    pub project_id: String,
    pub run_id: Uuid,
    pub task_id: String,
    pub failure_fingerprint: String,
    pub diagnosis_category: String,
    pub confidence: f64,
    pub risk_level: String,
    pub requires_approval: bool,
    pub original_actions: String,
    pub proposed_actions: String,
    pub ast_diff: String,
    pub rationale: String,
    pub evidence_refs: String,
    pub state: String,
    pub idempotency_key: String,
    pub healing_run_id: Option<Uuid>,
    pub verification_result: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub rejection_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// JSON: what the proposal needs to be executed later but has no column of its own (executor,
    /// environment, target URL, ...); see `0018_healing_proposal_context.sql`. `None` for a row
    /// written before that column existed.
    pub context: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealingStrategyMetricsRow {
    pub id: Uuid,
    pub project_id: String,
    pub strategy: String,
    pub failure_category: String,
    pub attempts_count: i64,
    pub verified_count: i64,
    pub not_verified_count: i64,
    pub regression_count: i64,
    pub calibrated_confidence: f64,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionTelemetryRow {
    pub id: Uuid,
    pub project_id: String,
    pub run_id: Uuid,
    pub task_id: String,
    pub task_name: String,
    pub test_case_id: Option<Uuid>,
    pub execution_id: Option<String>,
    pub executor: String,
    pub environment: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_ms: i64,
    pub status: String,
    pub retry_count: i64,
    pub healing_attempt_count: i64,
    pub failure_category: Option<String>,
    pub failure_fingerprint: Option<String>,
    pub error_message: Option<String>,
    pub evidence_refs: String,
    pub assertion_result: Option<String>,
    pub http_status: Option<i64>,
    pub selector: Option<String>,
    pub verification_outcome: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlakinessMetricsRow {
    pub id: Uuid,
    pub node_id: Uuid,
    pub flakiness_score: f64,
    pub window_runs: i64,
    pub window_failures: i64,
    pub environment_correlation: Option<String>,
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityNodeRow {
    pub id: Uuid,
    pub project_id: Option<String>,
    pub node_type: String,
    pub external_ref: Option<String>,
    pub name: String,
    pub metadata: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityRelationRow {
    pub id: Uuid,
    pub from_node_id: Uuid,
    pub to_node_id: Uuid,
    pub relation_type: String,
    pub metadata: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod history_tenant_tests {
    //! The tenant half of `task_history_by_name`'s project predicate.
    //!
    //! This lives inline here rather than in `store_tests.rs` only because of
    //! file ownership, not because it belongs to this file more: `store.rs` is
    //! where the SQL it pins lives, and the defect it guards (the non-sargable
    //! `COALESCE` rewrite) is invisible to `store_tests.rs::b3_task_history_
    //! does_not_cross_projects`, which only ever uses two *named* tenants.

    use super::*;
    use crate::domain::{RunPlan, SimOutcome, SimSpec, TaskDef};

    const SHARED: &str = "shared-task-name";

    /// A real on-disk store: the predicate under test is part of the SQL the
    /// file-backed store actually executes.
    async fn temp_store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.sqlite3");
        let store = Store::connect(&path).await.expect("connect");
        (store, dir)
    }

    fn plan(name: &str) -> RunPlan {
        RunPlan {
            name: name.into(),
            tasks: vec![TaskDef {
                id: "t1".into(),
                name: SHARED.into(),
                prerequisites: Vec::new(),
                simulate: Some(SimSpec {
                    outcome: SimOutcome::Pass,
                    delay_ms: 0,
                }),
                ..Default::default()
            }],
            concurrency: 1,
        }
    }

    /// One run of `SHARED` in `project`, started and finished at `status` —
    /// the two writes have to be separate, because `set_task_status` treats
    /// `set_started`/`set_finished` as mutually exclusive and a combined call
    /// would never set `finished_at`, which history reads filter on.
    async fn run_once(store: &Store, name: &str, project: Option<&str>, status: TaskStatus) {
        let rid = Uuid::new_v4();
        store
            .insert_run(
                rid,
                &plan(name),
                project,
                Some("tester"),
                RunState::Running,
                None,
            )
            .await
            .expect("insert run");
        store
            .set_task_status(rid, "t1", TaskStatus::Running, None, true, false)
            .await
            .expect("start task");
        store
            .set_task_status(rid, "t1", status, Some("because"), false, true)
            .await
            .expect("finish task");
    }

    fn count(rows: &[TaskHistoryRow], status: TaskStatus) -> usize {
        rows.iter().filter(|r| r.status == status).count()
    }

    /// The project predicate used to be `COALESCE(r.project_id, 'default') = ?`,
    /// which is non-sargable and had to be rewritten as an explicit disjunction
    /// to let `idx_runs_project_created` be used. That rewrite has to mean
    /// *exactly* the same thing, and the interesting half of "exactly" is the
    /// untenanted bucket: `runs.project_id IS NULL` belongs to the tenant
    /// `"default"`, and to no other. A rewrite that dropped the `IS NULL` arm
    /// would silently stop counting pre-tenant data; one that dropped the
    /// equality arm would hand `"default"` every untenanted run *and* every
    /// run explicitly filed under `"default"`, double-counting the former.
    #[tokio::test]
    async fn task_history_project_predicate_keeps_the_untenanted_bucket_scoped_to_default() {
        let (store, _dir) = temp_store().await;

        for i in 0..3 {
            run_once(&store, &format!("a{i}"), Some("alpha"), TaskStatus::Failed).await;
        }
        // The untenanted bucket: a run written before the tenant existed, whose
        // `project_id` really is NULL rather than the string "default".
        run_once(&store, "untenanted", None, TaskStatus::Passed).await;
        // A tenant that is literally named "default".
        run_once(&store, "named-default", Some("default"), TaskStatus::Passed).await;

        let as_default = store
            .task_history_by_name(SHARED, Some("default"), 20)
            .await
            .expect("default history");
        assert_eq!(
            as_default.len(),
            2,
            "the untenanted run and the run filed under \"default\" are both the \
             \"default\" tenant's, and neither may be counted twice"
        );
        assert_eq!(count(&as_default, TaskStatus::Passed), 2);

        let as_alpha = store
            .task_history_by_name(SHARED, Some("alpha"), 20)
            .await
            .expect("alpha history");
        assert_eq!(
            as_alpha.len(),
            3,
            "alpha owns three executions and must not see the untenanted one"
        );
        assert_eq!(count(&as_alpha, TaskStatus::Failed), 3);

        let unscoped = store
            .task_history_by_name(SHARED, None, 20)
            .await
            .expect("unscoped history");
        assert_eq!(
            unscoped.len(),
            5,
            "an unscoped caller is a global key and still sees every tenant"
        );
    }

    /// The `?2 = 'default'` arm is a literal comparison against a bind value, so
    /// it is worth pinning that a *named* tenant does not get the NULL bucket
    /// through the back door when that bind happens to be compared as NULL
    /// rather than as its own text.
    #[tokio::test]
    async fn task_history_of_a_named_tenant_never_includes_untenanted_runs() {
        let (store, _dir) = temp_store().await;

        run_once(&store, "untenanted", None, TaskStatus::Failed).await;
        run_once(&store, "beta", Some("beta"), TaskStatus::Passed).await;

        let beta = store
            .task_history_by_name(SHARED, Some("beta"), 20)
            .await
            .expect("beta history");
        assert_eq!(beta.len(), 1, "beta ran it once");
        assert_eq!(beta[0].status, TaskStatus::Passed);
    }
}

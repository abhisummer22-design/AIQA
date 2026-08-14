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
use sqlx::{Row, SqlitePool};
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone)]
pub struct Store {
    pub pool: SqlitePool,
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn parse_ts(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}

fn parse_ts_opt(s: Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
    s.map(|s| parse_ts(&s))
}

impl Store {
    pub async fn connect(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
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
        let plan_json = serde_json::to_string(plan)?;
        sqlx::query(
            "INSERT INTO runs (id, name, project_id, state, concurrency, plan, created_at, requested_by, policy_notes)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run_id.to_string())
        .bind(&plan.name)
        .bind(project_id)
        .bind(state.as_db_str())
        .bind(plan.concurrency as i64)
        .bind(plan_json)
        .bind(now_rfc3339())
        .bind(requested_by)
        .bind(policy_notes)
        .execute(&self.pool)
        .await?;

        for t in &plan.tasks {
            sqlx::query(
                "INSERT INTO tasks (id, run_id, name, status, executor_kind) VALUES (?, ?, ?, 'QUEUED', ?)",
            )
            .bind(&t.id)
            .bind(run_id.to_string())
            .bind(&t.name)
            .bind(&t.executor)
            .execute(&self.pool)
            .await?;
            for p in &t.prerequisites {
                sqlx::query(
                    "INSERT INTO task_dependencies (run_id, task_id, prerequisite_id) VALUES (?, ?, ?)",
                )
                .bind(run_id.to_string())
                .bind(&t.id)
                .bind(p)
                .execute(&self.pool)
                .await?;
            }
        }
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

    pub async fn set_run_finished(&self, run_id: RunId, state: RunState) -> Result<()> {
        sqlx::query("UPDATE runs SET state = ?, finished_at = ? WHERE id = ?")
            .bind(state.as_db_str())
            .bind(now_rfc3339())
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
            sqlx::query(
                "UPDATE tasks SET status = ?, started_at = ? WHERE run_id = ? AND id = ?",
            )
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

    pub async fn list_tasks_for_run(&self, run_id: RunId) -> Result<Vec<TaskState>> {
        let rows = sqlx::query(
            "SELECT id, name, status, reason, started_at, finished_at FROM tasks WHERE run_id = ? ORDER BY id",
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let deps = sqlx::query("SELECT task_id, prerequisite_id FROM task_dependencies WHERE run_id = ?")
            .bind(run_id.to_string())
            .fetch_all(&self.pool)
            .await?;
        let mut dep_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
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
                }
            })
            .collect())
    }

    /// Every run id known to the DB, most recent first — the "history" list
    /// that survives a restart, unlike the old in-memory-only `list_runs`.
    /// `project_filter`: `None` sees every project (admin/global keys);
    /// `Some(pid)` restricts to that project (SRD/PRD multi-tenancy gap).
    pub async fn list_run_ids(&self, project_filter: Option<&str>) -> Result<Vec<Uuid>> {
        let rows = if let Some(pid) = project_filter {
            sqlx::query("SELECT id FROM runs WHERE project_id = ? ORDER BY created_at DESC")
                .bind(pid)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query("SELECT id FROM runs ORDER BY created_at DESC")
                .fetch_all(&self.pool)
                .await?
        };
        Ok(rows
            .into_iter()
            .filter_map(|r| Uuid::parse_str(&r.get::<String, _>("id")).ok())
            .collect())
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

    /// Runs left in a non-terminal state — only possible if the process
    /// died mid-run (crash, kill, host restart). Used by the startup
    /// recovery pass in `main.rs`.
    pub async fn list_incomplete_runs(&self) -> Result<Vec<Uuid>> {
        let rows = sqlx::query(
            "SELECT id FROM runs WHERE state IN ('CREATED','SCHEDULED','RUNNING','PENDING_APPROVAL')",
        )
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
    pub async fn get_evidence_object_key(&self, id: Uuid) -> Result<Option<(String, Option<String>, Option<String>)>> {
        let row = sqlx::query("SELECT object_key, content_type, run_id FROM evidence WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| (r.get("object_key"), r.get("content_type"), r.get("run_id"))))
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
        Ok(row.map(|r| ApiKeyRow {
            id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
            name: r.get("name"),
            role: r.get("role"),
            project_id: r.get("project_id"),
        }))
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

    pub async fn revoke_api_key(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE api_keys SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
            .bind(now_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
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

    /// Execution history for every quality-node with this `external_ref`
    /// (i.e. every past run of a test with this name), oldest first. Used to
    /// compute flakiness (status flips) and historical failure rate.
    pub async fn task_history_by_name(&self, name: &str, limit: i64) -> Result<Vec<TaskHistoryRow>> {
        let rows = sqlx::query(
            "SELECT t.status as status, t.finished_at as finished_at
             FROM tasks t
             WHERE t.name = ? AND t.finished_at IS NOT NULL
             ORDER BY t.finished_at DESC LIMIT ?",
        )
        .bind(name)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
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
    pub async fn list_test_intelligence(&self, project_filter: Option<&str>) -> Result<Vec<TestIntelRow>> {
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
             ORDER BY r.score DESC",
        )
        .bind(pid)
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
    pub async fn upsert_failure_cluster(
        &self,
        project_id: &str,
        signature: &str,
        hypothesis: &str,
        confidence: f64,
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
            sqlx::query(
                "UPDATE failure_clusters SET occurrence_count = ?, last_seen_at = ?, reproducibility = ? WHERE id = ?",
            )
            .bind(count)
            .bind(now_rfc3339())
            .bind(reproducibility)
            .bind(&id)
            .execute(&self.pool)
            .await?;
            return Ok((Uuid::parse_str(&id).unwrap_or_default(), count));
        }
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO failure_clusters (id, project_id, signature, first_seen_at, last_seen_at, occurrence_count, root_cause_hypothesis, confidence, reproducibility, review_state)
             VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, 'NEEDS_REVIEW')",
        )
        .bind(id.to_string())
        .bind(project_id)
        .bind(signature)
        .bind(now_rfc3339())
        .bind(now_rfc3339())
        .bind(hypothesis)
        .bind(confidence)
        .bind(0.2_f64)
        .execute(&self.pool)
        .await?;
        Ok((id, 1))
    }

    pub async fn list_failure_clusters(&self, project_filter: Option<&str>) -> Result<Vec<FailureClusterRow>> {
        let pid = project_filter.unwrap_or("default");
        let rows = sqlx::query(
            "SELECT id, signature, first_seen_at, last_seen_at, occurrence_count, root_cause_hypothesis, confidence, reproducibility, review_state
             FROM failure_clusters WHERE project_id = ? ORDER BY occurrence_count DESC",
        )
        .bind(pid)
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
                root_cause_hypothesis: r.get("root_cause_hypothesis"),
                confidence: r.get("confidence"),
                reproducibility: r.get("reproducibility"),
                review_state: r.get("review_state"),
            })
            .collect())
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

    pub async fn list_bug_drafts(&self) -> Result<Vec<BugDraftRow>> {
        let rows = sqlx::query(
            "SELECT id, failure_cluster_id, title, description, severity, confidence, review_state, created_at, promoted_at, promoted_by
             FROM bug_drafts ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| BugDraftRow {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                failure_cluster_id: Uuid::parse_str(&r.get::<String, _>("failure_cluster_id")).unwrap_or_default(),
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
    pub async fn review_bug_draft(&self, id: Uuid, new_state: &str, reviewed_by: &str) -> Result<()> {
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

    pub async fn list_audit(&self, limit: i64) -> Result<Vec<AuditRow>> {
        let rows = sqlx::query(
            "SELECT occurred_at, actor, role, project_id, action, target, detail, outcome
             FROM audit_log ORDER BY occurred_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
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
}

// --------------------------------------------------------------- row DTOs

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
    pub root_cause_hypothesis: Option<String>,
    pub confidence: Option<f64>,
    pub reproducibility: Option<f64>,
    pub review_state: String,
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

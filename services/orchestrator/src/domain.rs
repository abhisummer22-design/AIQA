use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type TaskId = String;
pub type RunId = Uuid;

/// FRD §2 Test Status Model. Product failure and infrastructure failure are
/// kept as distinct classifications (Failed vs InfraError), per FRD §2 / SRD §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Queued,
    Running,
    Passed,
    Failed,
    Blocked,
    Skipped,
    Cancelled,
    Timeout,
    InfraError,
    NeedsReview,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, TaskStatus::Queued | TaskStatus::Running)
    }

    /// Whether a prerequisite in this state satisfies its dependents. Only a
    /// clean pass does (FRD §4: a failed/blocked/etc. prerequisite blocks
    /// dependents rather than letting them proceed).
    pub fn satisfies_dependents(&self) -> bool {
        matches!(self, TaskStatus::Passed)
    }

    /// SCREAMING_SNAKE_CASE string form, matching the serde representation —
    /// used by `store.rs` so the DB column and the wire format never drift.
    pub fn as_db_str(&self) -> &'static str {
        match self {
            TaskStatus::Queued => "QUEUED",
            TaskStatus::Running => "RUNNING",
            TaskStatus::Passed => "PASSED",
            TaskStatus::Failed => "FAILED",
            TaskStatus::Blocked => "BLOCKED",
            TaskStatus::Skipped => "SKIPPED",
            TaskStatus::Cancelled => "CANCELLED",
            TaskStatus::Timeout => "TIMEOUT",
            TaskStatus::InfraError => "INFRA_ERROR",
            TaskStatus::NeedsReview => "NEEDS_REVIEW",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "RUNNING" => TaskStatus::Running,
            "PASSED" => TaskStatus::Passed,
            "FAILED" => TaskStatus::Failed,
            "BLOCKED" => TaskStatus::Blocked,
            "SKIPPED" => TaskStatus::Skipped,
            "CANCELLED" => TaskStatus::Cancelled,
            "TIMEOUT" => TaskStatus::Timeout,
            "INFRA_ERROR" => TaskStatus::InfraError,
            "NEEDS_REVIEW" => TaskStatus::NeedsReview,
            _ => TaskStatus::Queued,
        }
    }
}

/// SRD §2 Orchestrator State Machine. Individual task states are independent
/// of overall run state.
///
/// `PendingApproval` and `Rejected` are additions beyond the original slice:
/// the policy engine (`policy.rs`) can hold a run here instead of scheduling
/// it immediately, when its plan trips a governance rule (PRD "governed
/// autonomous execution" / the critique's "no auth/roles ... presuppose
/// human-approval gates" finding). A held run only leaves `PendingApproval`
/// via an explicit `POST /runs/:id/approve` or `/reject` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunState {
    Created,
    PendingApproval,
    Rejected,
    Scheduled,
    Running,
    Partial,
    Completed,
    Cancelled,
    Failed,
}

impl RunState {
    /// BE-13 (§12.3) — states from which a run can still be cancelled are
    /// exactly the non-terminal ones: `PendingApproval` is deliberately
    /// excluded even though it isn't "finished" — that state already has its
    /// own dedicated stop action (`/reject`), so cancel targets a run that
    /// has actually entered the execution pipeline (`Created`/`Scheduled`
    /// while about to start, or `Running`).
    pub fn is_terminal(&self) -> bool {
        matches!(self, RunState::Rejected | RunState::Partial | RunState::Completed | RunState::Cancelled | RunState::Failed)
    }

    pub fn as_db_str(&self) -> &'static str {
        match self {
            RunState::Created => "CREATED",
            RunState::PendingApproval => "PENDING_APPROVAL",
            RunState::Rejected => "REJECTED",
            RunState::Scheduled => "SCHEDULED",
            RunState::Running => "RUNNING",
            RunState::Partial => "PARTIAL",
            RunState::Completed => "COMPLETED",
            RunState::Cancelled => "CANCELLED",
            RunState::Failed => "FAILED",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "PENDING_APPROVAL" => RunState::PendingApproval,
            "REJECTED" => RunState::Rejected,
            "SCHEDULED" => RunState::Scheduled,
            "RUNNING" => RunState::Running,
            "PARTIAL" => RunState::Partial,
            "COMPLETED" => RunState::Completed,
            "CANCELLED" => RunState::Cancelled,
            "FAILED" => RunState::Failed,
            _ => RunState::Created,
        }
    }
}

/// Stand-in outcome for the real Executor contract (SRD §16-17). Real adapters
/// (Playwright/Newman/HTTPX/Schemathesis/Maestro/...) are not implemented in
/// this slice; `simulate` lets a run plan declare what a task should do so the
/// continue-on-failure/dependency behavior (SRD §25) can be exercised and
/// tested deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimOutcome {
    Pass,
    Fail,
    Timeout,
    InfraError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimSpec {
    pub outcome: SimOutcome,
    #[serde(default = "default_delay_ms")]
    pub delay_ms: u64,
}

fn default_delay_ms() -> u64 {
    150
}

/// A task as submitted in a run plan. `executor` is the capability-routing
/// key (PRD §21 "capability routing") — "playwright" | "newman" | "pytest" |
/// "maestro" | absent (falls back to the SimulatedExecutor, driven by
/// `simulate`). `params` is executor-specific config (spec path, Postman
/// folder, pytest node id, Maestro flow file, ...) — intentionally untyped
/// since each adapter owns its own shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskDef {
    pub id: TaskId,
    pub name: String,
    #[serde(default)]
    pub prerequisites: Vec<TaskId>,
    #[serde(default)]
    pub simulate: Option<SimSpec>,
    #[serde(default)]
    pub executor: Option<String>,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPlan {
    pub name: String,
    pub tasks: Vec<TaskDef>,
    /// FRD §5: configurable concurrency per run.
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

fn default_concurrency() -> usize {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub id: TaskId,
    pub name: String,
    pub prerequisites: Vec<TaskId>,
    pub status: TaskStatus,
    pub reason: Option<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: RunId,
    pub name: String,
    pub state: RunState,
    pub total: usize,
    pub queued: usize,
    pub running: usize,
    pub passed: usize,
    pub failed: usize,
    pub blocked: usize,
    pub skipped: usize,
    pub cancelled: usize,
    pub timeout: usize,
    pub infra_error: usize,
    pub needs_review: usize,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Governance metadata (policy.rs) — populated when the run was held for
    /// human approval, empty for runs that were never gated.
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub requested_by: Option<String>,
    #[serde(default)]
    pub policy_notes: Option<String>,
    #[serde(default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub approved_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// SRD §4 event types. TaskStarted/…/TaskCompleted map onto FRD §2 statuses;
/// RunScheduled/RunCompleted bracket the whole run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    RunScheduled,
    TaskStarted,
    TaskPassed,
    TaskFailed,
    TaskBlocked,
    TaskSkipped,
    TaskCancelled,
    TaskTimeout,
    TaskInfraError,
    TaskNeedsReview,
    RunCompleted,
}

impl EventType {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            EventType::RunScheduled => "RUN_SCHEDULED",
            EventType::TaskStarted => "TASK_STARTED",
            EventType::TaskPassed => "TASK_PASSED",
            EventType::TaskFailed => "TASK_FAILED",
            EventType::TaskBlocked => "TASK_BLOCKED",
            EventType::TaskSkipped => "TASK_SKIPPED",
            EventType::TaskCancelled => "TASK_CANCELLED",
            EventType::TaskTimeout => "TASK_TIMEOUT",
            EventType::TaskInfraError => "TASK_INFRA_ERROR",
            EventType::TaskNeedsReview => "TASK_NEEDS_REVIEW",
            EventType::RunCompleted => "RUN_COMPLETED",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "TASK_STARTED" => EventType::TaskStarted,
            "TASK_PASSED" => EventType::TaskPassed,
            "TASK_FAILED" => EventType::TaskFailed,
            "TASK_BLOCKED" => EventType::TaskBlocked,
            "TASK_SKIPPED" => EventType::TaskSkipped,
            "TASK_CANCELLED" => EventType::TaskCancelled,
            "TASK_TIMEOUT" => EventType::TaskTimeout,
            "TASK_INFRA_ERROR" => EventType::TaskInfraError,
            "TASK_NEEDS_REVIEW" => EventType::TaskNeedsReview,
            "RUN_COMPLETED" => EventType::RunCompleted,
            _ => EventType::RunScheduled,
        }
    }
}

/// SRD §5 Event Ordering & Idempotency: every event carries event_id, run_id,
/// task_id, sequence, timestamp, schema_version. `sequence` is a per-run
/// monotonic counter assigned by the single writer (RunRecord), so consumers
/// can dedupe/reorder on reconnect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub event_id: Uuid,
    pub run_id: RunId,
    pub task_id: Option<TaskId>,
    pub sequence: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub schema_version: u32,
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub status: Option<TaskStatus>,
    pub summary: Option<String>,
    /// BE-14 (API Contract §14 envelope) — the run's owning project, filled
    /// in by `RunRecord::emit` from real, already-known state (never
    /// fabricated). `None` for an untenanted run, and also `None` for an
    /// event reconstructed from `run_events` via `list_events_since` — that
    /// table has no `project_id` column (denormalizing it there was not a
    /// genuine BE-14 requirement; see the BE-14 report), so a DB-reloaded
    /// event honestly omits it rather than guessing.
    #[serde(default)]
    pub project_id: Option<String>,
    /// BE-14 (§11.2 item 4 of the Integration Plan's own Realtime
    /// Strategy) — a *separate*, project-wide monotonic counter, distinct
    /// from `sequence` above (which stays run-local, exactly as every
    /// existing per-run catch-up/replay consumer already depends on). Only
    /// set on the copy published to a project's realtime stream
    /// (`AppState::project_stream`); `None` everywhere else — merging two
    /// concurrent runs' events by their own per-run `sequence` would
    /// collide (both start at 0), which is exactly the bug this field
    /// avoids for project-scoped subscribers.
    #[serde(default)]
    pub project_sequence: Option<u64>,
}

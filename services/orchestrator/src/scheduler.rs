use crate::domain::*;
use std::collections::{HashMap, HashSet};

/// Validate a plan before scheduling: unique ids, prerequisites reference real
/// tasks, and no dependency cycles (an implicit precondition of FRD §4 that
/// the spec doesn't call out but the scheduler can't safely run without).
pub fn validate_plan(plan: &RunPlan) -> Result<(), String> {
    if plan.tasks.is_empty() {
        return Err("plan must include at least one task".into());
    }
    let ids: HashSet<&str> = plan.tasks.iter().map(|t| t.id.as_str()).collect();
    if ids.len() != plan.tasks.len() {
        return Err("duplicate task ids in plan".into());
    }
    for t in &plan.tasks {
        for p in &t.prerequisites {
            if !ids.contains(p.as_str()) {
                return Err(format!(
                    "task '{}' declares unknown prerequisite '{}'",
                    t.id, p
                ));
            }
        }
    }

    let by_id: HashMap<&str, &TaskDef> = plan.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut mark: HashMap<&str, u8> = HashMap::new(); // 0 unvisited, 1 visiting, 2 done

    fn visit<'a>(
        id: &'a str,
        by_id: &HashMap<&'a str, &'a TaskDef>,
        mark: &mut HashMap<&'a str, u8>,
    ) -> Result<(), String> {
        match mark.get(id) {
            Some(2) => return Ok(()),
            Some(1) => return Err(format!("dependency cycle detected at task '{id}'")),
            _ => {}
        }
        mark.insert(id, 1);
        for p in &by_id[id].prerequisites {
            visit(p, by_id, mark)?;
        }
        mark.insert(id, 2);
        Ok(())
    }

    for t in &plan.tasks {
        visit(&t.id, &by_id, &mut mark)?;
    }
    Ok(())
}

/// Pure, synchronous dependency-DAG scheduler state (SRD §3). Holds no I/O —
/// the async driver in `engine.rs` wraps this to actually execute tasks and
/// emit events. Keeping this side-effect-free makes the continue-on-failure
/// and blocking semantics (FRD §3-4, acceptance test SRD §25) directly
/// unit-testable without spinning up the HTTP/WS stack.
pub struct RunEngine {
    pub tasks: HashMap<TaskId, TaskState>,
    order: Vec<TaskId>,
}

impl RunEngine {
    pub fn new(plan: &RunPlan) -> Self {
        let mut tasks = HashMap::new();
        let mut order = Vec::new();
        for t in &plan.tasks {
            order.push(t.id.clone());
            tasks.insert(
                t.id.clone(),
                TaskState {
                    id: t.id.clone(),
                    name: t.name.clone(),
                    prerequisites: t.prerequisites.clone(),
                    status: TaskStatus::Queued,
                    reason: None,
                    started_at: None,
                    finished_at: None,
                },
            );
        }
        Self { tasks, order }
    }

    /// Tasks that are QUEUED and whose prerequisites have all PASSED.
    /// Independent tasks (no prerequisites) are always eligible immediately,
    /// which is what lets them keep executing after unrelated failures
    /// (FRD §3 Continue-on-Failure Requirement).
    pub fn eligible(&self) -> Vec<TaskId> {
        self.order
            .iter()
            .filter(|id| {
                let t = &self.tasks[*id];
                t.status == TaskStatus::Queued
                    && t.prerequisites.iter().all(|p| {
                        self.tasks
                            .get(p)
                            .map(|pt| pt.status.satisfies_dependents())
                            .unwrap_or(false)
                    })
            })
            .cloned()
            .collect()
    }

    pub fn mark_started(&mut self, id: &str) {
        if let Some(t) = self.tasks.get_mut(id) {
            t.status = TaskStatus::Running;
            t.started_at = Some(chrono::Utc::now());
        }
    }

    /// Apply a terminal outcome to a task, then cascade BLOCKED to any queued
    /// dependents whose prerequisite did not pass, transitively (FRD §4).
    /// Returns the ids of every task newly made terminal by this call, in
    /// order (the completed task first, then any cascaded blocks).
    pub fn complete_task(
        &mut self,
        id: &str,
        status: TaskStatus,
        reason: Option<String>,
    ) -> Vec<TaskId> {
        let mut newly_terminal = Vec::new();
        if let Some(t) = self.tasks.get_mut(id) {
            t.status = status;
            t.reason = reason;
            t.finished_at = Some(chrono::Utc::now());
        }
        newly_terminal.push(id.to_string());
        self.cascade_blocks(&mut newly_terminal);
        newly_terminal
    }

    fn cascade_blocks(&mut self, newly_terminal: &mut Vec<TaskId>) {
        loop {
            let mut to_block: Vec<(TaskId, String)> = Vec::new();
            for id in &self.order {
                let t = &self.tasks[id];
                if t.status != TaskStatus::Queued || t.prerequisites.is_empty() {
                    continue;
                }
                for p in &t.prerequisites {
                    if let Some(pt) = self.tasks.get(p) {
                        if pt.status.is_terminal() && !pt.status.satisfies_dependents() {
                            to_block.push((
                                id.clone(),
                                format!(
                                    "prerequisite '{}' did not pass (status {:?})",
                                    p, pt.status
                                ),
                            ));
                            break;
                        }
                    }
                }
            }
            if to_block.is_empty() {
                break;
            }
            for (id, reason) in to_block {
                if let Some(t) = self.tasks.get_mut(&id) {
                    t.status = TaskStatus::Blocked;
                    t.reason = Some(reason);
                    t.finished_at = Some(chrono::Utc::now());
                }
                newly_terminal.push(id);
            }
        }
    }

    pub fn is_finished(&self) -> bool {
        self.tasks.values().all(|t| t.status.is_terminal())
    }

    pub fn counts(&self) -> HashMap<TaskStatus, usize> {
        let mut counts = HashMap::new();
        for t in self.tasks.values() {
            *counts.entry(t.status).or_insert(0) += 1;
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indep_task(id: &str, outcome: SimOutcome) -> TaskDef {
        TaskDef {
            id: id.into(),
            name: id.into(),
            prerequisites: vec![],
            simulate: Some(SimSpec {
                outcome,
                delay_ms: 0,
            }),
            ..Default::default()
        }
    }

    /// SRD §25 Acceptance Tests for Continue-on-Failure: "A suite with 10
    /// independent tests where test 2 and test 7 fail must still execute the
    /// other eligible tests and produce a final report containing all
    /// outcomes."
    #[test]
    fn srd_25_continue_on_failure_with_10_independent_tasks() {
        let tasks: Vec<TaskDef> = (1..=10)
            .map(|n| {
                let outcome = if n == 2 || n == 7 {
                    SimOutcome::Fail
                } else {
                    SimOutcome::Pass
                };
                indep_task(&format!("t{n}"), outcome)
            })
            .collect();
        let plan = RunPlan {
            name: "srd-25".into(),
            tasks,
            concurrency: 4,
        };
        validate_plan(&plan).unwrap();
        let mut engine = RunEngine::new(&plan);

        // All 10 are independent, so all are eligible up front and remain so
        // even after some are simulated as failing below.
        let eligible = engine.eligible();
        assert_eq!(eligible.len(), 10, "all independent tasks are eligible immediately");

        for id in &eligible {
            engine.mark_started(id);
        }
        for n in 1..=10 {
            let id = format!("t{n}");
            let status = if n == 2 || n == 7 {
                TaskStatus::Failed
            } else {
                TaskStatus::Passed
            };
            let newly = engine.complete_task(&id, status, None);
            // No dependents in this plan, so completing a task never cascades.
            assert_eq!(newly, vec![id]);
        }

        assert!(engine.is_finished());
        let counts = engine.counts();
        assert_eq!(counts.get(&TaskStatus::Passed).copied().unwrap_or(0), 8);
        assert_eq!(counts.get(&TaskStatus::Failed).copied().unwrap_or(0), 2);
        assert_eq!(engine.tasks["t2"].status, TaskStatus::Failed);
        assert_eq!(engine.tasks["t7"].status, TaskStatus::Failed);
        for n in [1, 3, 4, 5, 6, 8, 9, 10] {
            assert_eq!(engine.tasks[&format!("t{n}")].status, TaskStatus::Passed);
        }
    }

    /// SRD §25: "A dependent test must become BLOCKED when its prerequisite
    /// fails." Also covers transitive blocking (grandchild task).
    #[test]
    fn dependent_tasks_become_blocked_when_prerequisite_fails() {
        let plan = RunPlan {
            name: "dep-chain".into(),
            tasks: vec![
                indep_task("a", SimOutcome::Fail),
                TaskDef {
                    id: "b".into(),
                    name: "b".into(),
                    prerequisites: vec!["a".into()],
                    simulate: Some(SimSpec {
                        outcome: SimOutcome::Pass,
                        delay_ms: 0,
                    }),
                    ..Default::default()
                },
                TaskDef {
                    id: "c".into(),
                    name: "c".into(),
                    prerequisites: vec!["b".into()],
                    simulate: Some(SimSpec {
                        outcome: SimOutcome::Pass,
                        delay_ms: 0,
                    }),
                    ..Default::default()
                },
                indep_task("d", SimOutcome::Pass), // independent, must still run
            ],
            concurrency: 4,
        };
        validate_plan(&plan).unwrap();
        let mut engine = RunEngine::new(&plan);

        // Only 'a' and 'd' are eligible up front; 'b' and 'c' wait on their
        // prerequisites.
        let mut eligible = engine.eligible();
        eligible.sort();
        assert_eq!(eligible, vec!["a".to_string(), "d".to_string()]);

        engine.mark_started("a");
        engine.mark_started("d");
        let newly = engine.complete_task("a", TaskStatus::Failed, Some("boom".into()));
        // Completing 'a' as Failed must cascade-block both 'b' and 'c'.
        assert_eq!(newly, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!(engine.tasks["b"].status, TaskStatus::Blocked);
        assert_eq!(engine.tasks["c"].status, TaskStatus::Blocked);

        // 'd' is independent and must remain eligible/unaffected by 'a' failing.
        assert_eq!(engine.tasks["d"].status, TaskStatus::Running);
        engine.complete_task("d", TaskStatus::Passed, None);
        assert_eq!(engine.tasks["d"].status, TaskStatus::Passed);

        assert!(engine.is_finished());
    }

    #[test]
    fn validate_plan_rejects_cycles_and_unknown_prerequisites() {
        let cyclic = RunPlan {
            name: "cyclic".into(),
            tasks: vec![
                TaskDef {
                    id: "a".into(),
                    name: "a".into(),
                    prerequisites: vec!["b".into()],
                    ..Default::default()
                },
                TaskDef {
                    id: "b".into(),
                    name: "b".into(),
                    prerequisites: vec!["a".into()],
                    ..Default::default()
                },
            ],
            concurrency: 1,
        };
        assert!(validate_plan(&cyclic).is_err());

        let unknown_prereq = RunPlan {
            name: "unknown".into(),
            tasks: vec![TaskDef {
                id: "a".into(),
                name: "a".into(),
                prerequisites: vec!["ghost".into()],
                ..Default::default()
            }],
            concurrency: 1,
        };
        assert!(validate_plan(&unknown_prereq).is_err());
    }
}

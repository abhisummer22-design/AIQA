/**
 * Phase 8C — what the scheduled-regression page reads, and how often: the project's schedules (their next run and last result), the
 * history of the schedule that is open (quicker while one of its runs is going) and the report of the run that is open (every couple of
 * seconds until the run has ended, then not again by itself).
 */
import { aiqaApi } from '../services/aiqaApi';
import type { RegressionReport, RegressionRunAccount, RegressionSchedule } from '../types/contract';
import { RETRY_DELAY_MS, REPORT_POLL_MS, RUNS_IDLE_POLL_MS, RUNS_POLL_MS, SCHEDULE_POLL_MS } from '../components/regression/regressionModel';
import { usePolled, type Polled } from './usePolled';

export function useRegressionSchedules(projectId: string | null): Polled<RegressionSchedule[]> {
  return usePolled(
    projectId ?? '',
    (signal) => aiqaApi.regression.schedules.list(projectId as string, signal).then((r) => r.schedules),
    (_value, failed) => (failed ? RETRY_DELAY_MS : SCHEDULE_POLL_MS),
  );
}

export function useRegressionRuns(projectId: string | null, scheduleId: string | null): Polled<RegressionRunAccount[]> {
  const key = projectId && scheduleId ? `${projectId}/${scheduleId}` : '';
  return usePolled(
    key,
    (signal) => aiqaApi.regression.schedules.runs(projectId as string, scheduleId as string, signal).then((r) => r.runs),
    (runs, failed) => (failed ? RETRY_DELAY_MS : runs?.some((r) => !r.final) ? RUNS_POLL_MS : RUNS_IDLE_POLL_MS),
  );
}

export function useRegressionReport(projectId: string | null, runId: string | null): Polled<RegressionReport> {
  const key = projectId && runId ? `${projectId}/${runId}` : '';
  return usePolled(
    key,
    (signal) => aiqaApi.regression.runs.get(projectId as string, runId as string, signal),
    (report, failed) => (failed ? RETRY_DELAY_MS : report && !report.final ? REPORT_POLL_MS : null),
  );
}

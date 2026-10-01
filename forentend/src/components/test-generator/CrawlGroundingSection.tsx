/**
 * Real crawl grounding for generation (Phase 6 -> Phase 8 wiring).
 *
 * Phase 6 built a real crawler + per-role visibility matrix
 * (`POST .../crawl` -> `GET .../crawl/:run_id/result`, `crawler.rs`) with no
 * consumer wired up — the backend could crawl a real, running application,
 * but nothing in this app ever looked at what it found. This section is
 * that missing consumer: it runs a real crawl, waits for it to genuinely
 * finish through the exact same run/task pipeline every other browser task
 * uses, and — once the caller opts in via the checkbox below — passes the
 * real `run_id` to `POST .../test-cases/generate-ai` as `crawl_run_id`, so
 * real discovered pages/elements ground generation instead of only a
 * written requirement.
 *
 * The optional "login roles" builder below lets a caller supply a real
 * login sequence (NAVIGATE/FILL/CLICK — `crawler::CrawlRole.login_actions`'
 * own closed vocabulary, the same one a compiled test case's browser
 * actions already use) so the crawl can authenticate as a real role instead
 * of only ever seeing anonymous/public pages. The anonymous role is always
 * included alongside any authenticated ones — `CrawlOutput.visibility_matrix`
 * (`crawler.rs`) is specifically built to diff what different roles can see,
 * which needs more than one role's real result to say anything.
 *
 * Every status shown here is the real run's own state
 * (`aiqaApi.executions.get`) — `RUNNING` while genuinely in flight,
 * `FAILED`/`CANCELLED` (or a task's own real `reason`) shown as a real
 * failure, never silently retried into a fabricated success. Polling is a
 * plain, bounded HTTP poll (not this app's realtime WebSocket layer) —
 * deliberately kept simple for this one-off, short-lived, user-initiated
 * action rather than wiring a new consumer onto `RealtimeContext`.
 */
import React, { useEffect, useRef, useState } from 'react';
import { Compass, Loader2, CheckCircle2, XCircle, Info, KeyRound, Plus, Trash2, Eye, EyeOff } from 'lucide-react';
import { AppSourceType, AppSourceConnection } from '../../types';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import type { CrawlOutput, CrawlRole, PlanAction } from '../../types/contract';

/** A role of a finished crawl, as the compile controls list it. */
export interface CrawlRoleOption {
  name: string;
  /** `false` when the crawl could not log in as this role; `undefined` when the result does not say. */
  loggedIn?: boolean;
}

/** The finished crawl the page can compile test cases against (Phase 5): its run id and the roles it found. */
export interface GroundingCrawl {
  runId: string;
  roles: CrawlRoleOption[];
}

/** `crawler.rs` sends the crawl result in camelCase (`loginSucceeded`); the contract type names it in snake_case.
 *  Either is read, so a role is never wrongly shown as one that could not log in. */
function toRoleOption(role: CrawlOutput['roles'][number]): CrawlRoleOption {
  const loggedIn = (role as { loginSucceeded?: boolean }).loginSucceeded ?? role.login_succeeded;
  return { name: role.role, loggedIn };
}

type CrawlStatus = 'IDLE' | 'STARTING' | 'RUNNING' | 'COMPLETED' | 'FAILED';

type LoginStepType = 'NAVIGATE' | 'FILL' | 'CLICK';

/** A login step still being edited — `url`/`selector`/`value` are always
 *  present so inputs stay controlled regardless of which `type` is
 *  currently selected; only the fields the chosen `type` actually needs are
 *  read when converting to a real `PlanAction` (`toPlanAction` below).
 *  `isSecret` is UI-only (masks the value input) and is never sent — the
 *  real value itself is sent in plain JSON either way, same as any other
 *  FILL action this app already compiles (disclosed in the notice below the
 *  builder, mirroring `crawler.rs`'s own module doc comment on this). */
interface LoginStepDraft {
  localId: number;
  type: LoginStepType;
  url: string;
  selector: string;
  value: string;
  isSecret: boolean;
}

interface AuthRoleDraft {
  localId: number;
  name: string;
  steps: LoginStepDraft[];
  /** 'quick' shows only Login page URL / Username / Password — the real selectors used to find those
   *  fields are the QUICK_*_SELECTOR guesses below, never shown or editable. 'advanced' is the original
   *  NAVIGATE/FILL/CLICK builder (with the captions above), for a login page the guesses do not fit.
   *  Both modes read and write the exact same `steps`, so switching between them never loses anything
   *  already typed. */
  mode: 'quick' | 'advanced';
}

/** A login value written `env:NAME` is read from the orchestrator's environment when the crawl (or a test compiled from
 *  it) runs — nothing is embedded or stored. Only these names can be referenced (`execution_plan.rs::secret_name`). */
const SECRET_REFERENCE = /^env:\s*(AIQA_LOGIN_|AIQA_SECRET_)[A-Za-z0-9_]*\s*$/;

/** Plain-English caption for the first four steps of a freshly-added role
 *  (`addAuthRole` below always creates exactly this NAVIGATE/FILL/FILL/CLICK
 *  shape) — shown above the row so a reader does not need to know what
 *  "NAVIGATE"/"FILL"/"CLICK" mean to fill the form in. Any step added beyond
 *  these four (`addStep`) is a custom/advanced one with no fixed meaning, so
 *  it keeps the plain numbered label instead. The dropdown and inputs below
 *  are unchanged either way — this is a label only, never a restriction. */
const DEFAULT_STEP_CAPTIONS = ['Login page', 'Username', 'Password', 'Login button'];

function stepCaption(idx: number): string | null {
  return DEFAULT_STEP_CAPTIONS[idx] ?? null;
}

function selectorPlaceholder(caption: string | null): string {
  switch (caption) {
    case 'Username':
      return '#username or input[name=email]';
    case 'Password':
      return '#password';
    case 'Login button':
      return 'button[type=submit]';
    default:
      return '#selector';
  }
}

/** Quick mode's best-effort guesses for a standard single-page login form — the same handful of
 *  patterns a password manager's own autofill relies on. `input[type="password"]` in particular is
 *  reliable on almost any real login page. Any one of these can still be wrong for an unusual login
 *  page; "Advanced" shows these exact values (pre-filled, already in use) so they can be corrected
 *  instead of guessed at blind. */
const QUICK_USERNAME_SELECTOR =
  'input[type="email"], input[name="username"], input[name="email"], input[id="username"], input[id="email"], input[type="text"]';
const QUICK_PASSWORD_SELECTOR = 'input[type="password"]';
const QUICK_SUBMIT_SELECTOR = 'button[type="submit"], input[type="submit"]';

function toPlanAction(step: LoginStepDraft): PlanAction {
  switch (step.type) {
    case 'NAVIGATE':
      return { type: 'NAVIGATE', url: step.url.trim() };
    case 'CLICK':
      return { type: 'CLICK', selector: step.selector.trim() };
    case 'FILL':
      return { type: 'FILL', selector: step.selector.trim(), value: step.value };
  }
}

function toCrawlRole(role: AuthRoleDraft): CrawlRole {
  return { name: role.name.trim(), login_actions: role.steps.map(toPlanAction) };
}

/** Client-side only — the backend's own `validate_crawl_plan` is the real
 *  authority (duplicate role names, empty role name, non-browser actions)
 *  and its real rejection is what actually surfaces on a genuine mismatch.
 *  This just catches the one thing the backend does NOT check (an empty
 *  url/selector/value would pass its validation as a structurally-valid
 *  action and only fail confusingly once the crawl actually runs) before a
 *  doomed crawl ever starts. */
function authRolesValidationError(roles: AuthRoleDraft[]): string | null {
  for (const role of roles) {
    if (!role.name.trim()) return 'Every login role needs a name.';
    if (role.steps.length === 0) return `Role "${role.name}" needs at least one login step.`;
    // Quick mode never shows a selector, so its own errors read as its own three fields, not as
    // NAVIGATE/FILL jargon it never showed. Its selectors are always the QUICK_*_SELECTOR defaults,
    // so nothing below in this branch needs to check them.
    if (role.mode === 'quick' && role.steps.length === 4) {
      const [navigate, username, password] = role.steps;
      if (!navigate.url.trim()) return `Role "${role.name}": enter the login page URL.`;
      if (!username.value.trim()) return `Role "${role.name}": enter a username or email.`;
      if (username.value.startsWith('env:') && !SECRET_REFERENCE.test(username.value)) {
        return `Role "${role.name}": a value starting env: must name a variable that starts AIQA_LOGIN_ or AIQA_SECRET_ (for example env:AIQA_LOGIN_ADMIN_PASSWORD).`;
      }
      if (!password.value) return `Role "${role.name}": enter a password (or an env: reference).`;
      if (password.value.startsWith('env:') && !SECRET_REFERENCE.test(password.value)) {
        return `Role "${role.name}": a value starting env: must name a variable that starts AIQA_LOGIN_ or AIQA_SECRET_ (for example env:AIQA_LOGIN_ADMIN_PASSWORD).`;
      }
      continue;
    }
    for (const step of role.steps) {
      if (step.type === 'NAVIGATE' && !step.url.trim()) {
        return `Role "${role.name}": a NAVIGATE step needs a URL.`;
      }
      if ((step.type === 'FILL' || step.type === 'CLICK') && !step.selector.trim()) {
        return `Role "${role.name}": a ${step.type} step needs a selector.`;
      }
      if (step.type === 'FILL' && !step.value) {
        return `Role "${role.name}": a FILL step needs a value.`;
      }
      if (step.type === 'FILL' && step.value.startsWith('env:') && !SECRET_REFERENCE.test(step.value)) {
        return `Role "${role.name}": a value starting env: must name a variable that starts AIQA_LOGIN_ or AIQA_SECRET_ (for example env:AIQA_LOGIN_ADMIN_PASSWORD).`;
      }
    }
  }
  return null;
}

interface QuickLoginFieldsProps {
  role: AuthRoleDraft;
  updateStep: (roleId: number, stepId: number, patch: Partial<LoginStepDraft>) => void;
  onSwitchToAdvanced: () => void;
  disabled: boolean;
}

/** The three fields a normal login needs: Login page URL, Username/email, Password — writing
 *  straight into the exact same `steps` the advanced builder edits (`role.steps[0].url`,
 *  `role.steps[1].value`, `role.steps[2].value`). Their selectors are never shown here — they were
 *  already set to the QUICK_*_SELECTOR guesses by `addAuthRole` and stay that way unless "Advanced"
 *  is used to change them. Only rendered while `role.steps.length === 4` (its caller checks this),
 *  so the fixed [0]/[1]/[2] indexing below is always the real NAVIGATE/username-FILL/password-FILL. */
const QuickLoginFields: React.FC<QuickLoginFieldsProps> = ({ role, updateStep, onSwitchToAdvanced, disabled }) => {
  const [navigateStep, usernameStep, passwordStep] = role.steps;
  return (
    <div className="space-y-2 pl-2 border-l border-slate-800">
      <div className="space-y-1">
        <label className="text-[10px] font-mono text-content-muted uppercase tracking-wide">Login page URL</label>
        <input
          type="text"
          placeholder="https://your-app.example.com/login"
          value={navigateStep.url}
          onChange={(e) => updateStep(role.localId, navigateStep.localId, { url: e.target.value })}
          disabled={disabled}
          className="w-full px-2 py-1 rounded bg-[#0B1120] border border-slate-700 text-[11px] font-mono text-slate-100 placeholder-slate-500 focus:outline-none"
        />
      </div>
      <div className="space-y-1">
        <label className="text-[10px] font-mono text-content-muted uppercase tracking-wide">Username / email</label>
        <input
          type="text"
          placeholder="you@example.com"
          value={usernameStep.value}
          onChange={(e) => updateStep(role.localId, usernameStep.localId, { value: e.target.value })}
          disabled={disabled}
          className="w-full px-2 py-1 rounded bg-[#0B1120] border border-slate-700 text-[11px] font-mono text-slate-100 placeholder-slate-500 focus:outline-none"
        />
      </div>
      <div className="space-y-1">
        <label className="text-[10px] font-mono text-content-muted uppercase tracking-wide">Password</label>
        <div className="flex items-center gap-1">
          <input
            type={passwordStep.isSecret ? 'password' : 'text'}
            placeholder="your password, or env:AIQA_LOGIN_ADMIN_PASSWORD"
            value={passwordStep.value}
            onChange={(e) => updateStep(role.localId, passwordStep.localId, { value: e.target.value })}
            disabled={disabled}
            className="flex-1 min-w-0 px-2 py-1 rounded bg-[#0B1120] border border-slate-700 text-[11px] font-mono text-slate-100 placeholder-slate-500 focus:outline-none"
          />
          <button
            type="button"
            onClick={() => updateStep(role.localId, passwordStep.localId, { isSecret: !passwordStep.isSecret })}
            title={passwordStep.isSecret ? 'Value hidden — click to reveal while typing' : 'Click to mask this value'}
            className="p-1 rounded text-content-muted hover:text-slate-300 shrink-0 cursor-pointer"
          >
            {passwordStep.isSecret ? <EyeOff className="w-3 h-3" /> : <Eye className="w-3 h-3" />}
          </button>
        </div>
      </div>
      <button
        type="button"
        onClick={onSwitchToAdvanced}
        disabled={disabled}
        className="text-[10px] font-mono text-teal-400 hover:text-teal-300 disabled:opacity-50 cursor-pointer"
      >
        Advanced (customize exactly what it clicks/fills) →
      </button>
    </div>
  );
};

interface CrawlGroundingSectionProps {
  appSources: Record<AppSourceType, AppSourceConnection>;
  projectId: string | null;
  /** The real `run_id` currently opted into generation, or `null` when no
   *  crawl has completed or the caller unchecked "use it". Controlled by
   *  the parent so `handleTriggerGenerate` can read it directly. */
  crawlRunId: string | null;
  onCrawlRunIdChange: (runId: string | null) => void;
  /** Phase 5 — the finished crawl on show (or `null`), with its roles, so a test case can be compiled against it. Told
   *  regardless of whether the crawl is also chosen for generation. */
  onCrawlReady?: (crawl: GroundingCrawl | null) => void;
  disabled?: boolean;
}

const POLL_INTERVAL_MS = 2000;
const MAX_POLL_ATTEMPTS = 45; // ~90s — a real, disclosed bound, not an infinite wait.
const TERMINAL_RUN_STATES = new Set(['COMPLETED', 'PARTIAL', 'FAILED', 'CANCELLED']);

export const CrawlGroundingSection: React.FC<CrawlGroundingSectionProps> = ({
  appSources,
  projectId,
  crawlRunId,
  onCrawlRunIdChange,
  onCrawlReady,
  disabled = false,
}) => {
  const connectedUrls = (Object.values(appSources) as AppSourceConnection[]).filter(
    (a) => a.status === 'CONNECTED' && a.type !== 'FIGMA',
  );
  const [targetUrl, setTargetUrl] = useState(connectedUrls[0]?.url ?? '');
  const [status, setStatus] = useState<CrawlStatus>('IDLE');
  const [error, setError] = useState<string | null>(null);
  const [output, setOutput] = useState<CrawlOutput | null>(null);
  const [lastRunId, setLastRunId] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const [authRoles, setAuthRoles] = useState<AuthRoleDraft[]>([]);
  // Phase 6: off unless ticked, so a crawl is exactly what it always was.
  const [focusRequirements, setFocusRequirements] = useState(false);
  const nextLocalId = useRef(0);

  const addAuthRole = () => {
    const roleId = nextLocalId.current++;
    setAuthRoles((prev) => [
      ...prev,
      {
        localId: roleId,
        name: '',
        // Quick by default — the whole point of the four steps below already being filled in with a
        // best-effort real selector is that a caller who trusts the guess never has to see them.
        mode: 'quick',
        steps: [
          { localId: nextLocalId.current++, type: 'NAVIGATE', url: targetUrl, selector: '', value: '', isSecret: false },
          {
            localId: nextLocalId.current++,
            type: 'FILL',
            url: '',
            selector: QUICK_USERNAME_SELECTOR,
            value: '',
            isSecret: false,
          },
          {
            localId: nextLocalId.current++,
            type: 'FILL',
            url: '',
            selector: QUICK_PASSWORD_SELECTOR,
            value: '',
            isSecret: true,
          },
          {
            localId: nextLocalId.current++,
            type: 'CLICK',
            url: '',
            selector: QUICK_SUBMIT_SELECTOR,
            value: '',
            isSecret: false,
          },
        ],
      },
    ]);
  };

  const removeAuthRole = (roleId: number) => {
    setAuthRoles((prev) => prev.filter((r) => r.localId !== roleId));
  };

  const updateAuthRoleName = (roleId: number, name: string) => {
    setAuthRoles((prev) => prev.map((r) => (r.localId === roleId ? { ...r, name } : r)));
  };

  const setRoleMode = (roleId: number, mode: 'quick' | 'advanced') => {
    setAuthRoles((prev) => prev.map((r) => (r.localId === roleId ? { ...r, mode } : r)));
  };

  const addStep = (roleId: number) => {
    setAuthRoles((prev) =>
      prev.map((r) =>
        r.localId === roleId
          ? {
              ...r,
              steps: [
                ...r.steps,
                { localId: nextLocalId.current++, type: 'FILL', url: '', selector: '', value: '', isSecret: false },
              ],
            }
          : r,
      ),
    );
  };

  const removeStep = (roleId: number, stepId: number) => {
    setAuthRoles((prev) =>
      prev.map((r) => (r.localId === roleId ? { ...r, steps: r.steps.filter((s) => s.localId !== stepId) } : r)),
    );
  };

  const updateStep = (roleId: number, stepId: number, patch: Partial<LoginStepDraft>) => {
    setAuthRoles((prev) =>
      prev.map((r) =>
        r.localId === roleId
          ? { ...r, steps: r.steps.map((s) => (s.localId === stepId ? { ...s, ...patch } : s)) }
          : r,
      ),
    );
  };

  useEffect(() => {
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, []);

  // Which crawl a test case can be compiled against: the finished one on show, with the roles it found.
  useEffect(() => {
    onCrawlReady?.(output && lastRunId ? { runId: lastRunId, roles: output.roles.map(toRoleOption) } : null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [output, lastRunId]);

  // `crawlRunId` is the parent's persisted selection; `output`/`status` are
  // this component's own local state, lost on unmount (e.g. switching away
  // from this branch and back). Without this, a real, still-selected crawl
  // would silently keep being sent to generation with no visible summary or
  // way to un-check it — restore the real summary from the real result
  // instead of leaving that mismatch.
  useEffect(() => {
    if (!crawlRunId || !projectId || output) return;
    let cancelled = false;
    let settled = false;
    setStatus('RUNNING');
    aiqaApi.crawl
      .result(projectId, crawlRunId)
      .then((result) => {
        settled = true;
        if (cancelled) return;
        setOutput(result);
        setLastRunId(crawlRunId);
        setStatus('COMPLETED');
      })
      .catch(() => {
        settled = true;
        if (cancelled) return;
        // The selection still exists in the parent, but its real summary
        // could not be re-read — an honest idle state, not a fabricated one.
        setStatus('IDLE');
      });
    return () => {
      cancelled = true;
      // Torn down before its request answered (the parent cleared
      // `crawlRunId`, or `projectId` changed — a project switch), so neither
      // handler above will ever run: hand back the 'RUNNING' set above, or
      // the button stays disabled as "Crawling…". Once it has answered, the
      // status is no longer this effect's to reset — `handleCrawl` may own it.
      if (!settled) setStatus('IDLE');
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [crawlRunId, projectId]);

  const stopPolling = () => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  };

  const handleCrawl = async () => {
    if (!projectId) {
      setError('Select a project before crawling — the real crawl endpoint is project-scoped.');
      return;
    }
    const url = targetUrl.trim();
    if (!url) {
      setError('Enter or pick an application URL to crawl.');
      return;
    }
    const rolesError = authRolesValidationError(authRoles);
    if (rolesError) {
      setError(rolesError);
      return;
    }

    setError(null);
    setOutput(null);
    onCrawlRunIdChange(null);
    setStatus('STARTING');

    try {
      const started = await aiqaApi.crawl.start(projectId, {
        base_url: url,
        // Anonymous is always included alongside any real login roles below
        // — `CrawlOutput.visibility_matrix` needs more than one role's real
        // result to say what's role-gated.
        roles: [{ name: 'anonymous', login_actions: [] }, ...authRoles.map(toCrawlRole)],
        ...(focusRequirements ? { focus_requirements: true } : {}),
      });
      setLastRunId(started.run_id);
      setStatus('RUNNING');

      let attempts = 0;
      pollRef.current = setInterval(async () => {
        attempts += 1;
        try {
          const run = await aiqaApi.executions.get(projectId, started.run_id);
          if (TERMINAL_RUN_STATES.has(run.state)) {
            stopPolling();
            if (run.state === 'COMPLETED' || run.state === 'PARTIAL') {
              try {
                const result = await aiqaApi.crawl.result(projectId, started.run_id);
                setOutput(result);
                setStatus('COMPLETED');
                onCrawlRunIdChange(started.run_id);
              } catch (err) {
                setStatus('FAILED');
                setError(err instanceof ApiError ? err.message : 'Crawl finished but its real result could not be read.');
              }
            } else {
              const failedTask = run.tasks.find((t) => t.reason);
              setStatus('FAILED');
              setError(failedTask?.reason ?? `Crawl did not complete (${run.state}).`);
            }
            return;
          }
          if (attempts >= MAX_POLL_ATTEMPTS) {
            stopPolling();
            setStatus('FAILED');
            setError('Timed out waiting for the crawl to finish — it may still be running; check the Execution Hub.');
          }
        } catch (err) {
          stopPolling();
          setStatus('FAILED');
          setError(err instanceof ApiError ? err.message : 'Could not check the crawl run status.');
        }
      }, POLL_INTERVAL_MS);
    } catch (err) {
      setStatus('FAILED');
      setError(err instanceof ApiError ? err.message : 'Failed to start the crawl.');
    }
  };

  const totalPages = output ? new Set(output.roles.flatMap((r) => r.pages.map((p) => p.url))).size : 0;
  const totalElements = output ? output.roles.reduce((sum, r) => sum + r.pages.reduce((s, p) => s + p.elements.length, 0), 0) : 0;
  const isBusy = status === 'STARTING' || status === 'RUNNING';

  return (
    <div id="crawl-grounding-section" className="space-y-3">
      <div className="pb-2 border-b border-slate-800/80">
        <div className="flex items-center gap-2">
          <Compass className="w-4 h-4 text-teal-400" />
          <h2 className="text-sm font-bold text-slate-100 font-sans tracking-tight">Crawl for Real UI Grounding</h2>
          <span className="text-[10px] font-mono font-semibold px-2 py-0.5 rounded bg-teal-950/60 border border-teal-800/60 text-teal-300">
            OPTIONAL
          </span>
        </div>
        <p className="text-xs text-slate-400 font-sans mt-0.5">
          Run a real browser crawl of the application and feed the real pages/elements it finds into generation —
          anonymous by default, or add a real login sequence below to crawl as an authenticated role too.
        </p>
      </div>

      <div className="flex items-center gap-1.5">
        <input
          id="crawl-target-url"
          type="url"
          placeholder="https://your-app.example.com"
          value={targetUrl}
          onChange={(e) => setTargetUrl(e.target.value)}
          disabled={disabled || isBusy}
          list="crawl-connected-urls"
          className="flex-1 px-2.5 py-1.5 rounded bg-[#020617] border border-slate-700 hover:border-slate-600 focus:border-teal-500 text-xs font-mono text-slate-100 placeholder-slate-500 focus:outline-none"
        />
        <datalist id="crawl-connected-urls">
          {connectedUrls.map((s) => (
            <option key={s.type} value={s.url} />
          ))}
        </datalist>
        <button
          type="button"
          id="btn-start-crawl"
          onClick={handleCrawl}
          disabled={disabled || isBusy || !projectId || !targetUrl.trim()}
          className="px-3 py-1.5 rounded bg-teal-700 hover:bg-teal-600 disabled:bg-slate-800 text-white disabled:text-content-muted text-xs font-mono font-medium cursor-pointer transition-colors flex items-center gap-1 shrink-0"
        >
          {isBusy ? (
            <>
              <Loader2 className="w-3 h-3 animate-spin" />
              <span>{status === 'STARTING' ? 'Starting…' : 'Crawling…'}</span>
            </>
          ) : (
            <span>Crawl This Application</span>
          )}
        </button>
      </div>

      <label className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] text-slate-300 cursor-pointer w-fit">
        <input
          id="crawl-focus-requirements"
          type="checkbox"
          checked={focusRequirements}
          onChange={(e) => setFocusRequirements(e.target.checked)}
          disabled={disabled || isBusy}
          className="accent-teal-500"
        />
        <span>Visit the pages the requirements mention first</span>
        <span className="text-[10px] text-content-muted">
          Needs an uploaded requirements document. On a large application the crawl&apos;s page limit is spent on those pages first.
        </span>
      </label>

      <div className="space-y-2">
        {authRoles.length === 0 ? (
          <button
            type="button"
            id="btn-add-login-role"
            onClick={addAuthRole}
            disabled={disabled || isBusy}
            className="flex items-center gap-1.5 text-[11px] font-mono text-amber-400 hover:text-amber-300 disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
          >
            <Plus className="w-3 h-3" />
            <KeyRound className="w-3 h-3" />
            <span>Add a login role (optional)</span>
          </button>
        ) : (
          <div className="space-y-2">
            {authRoles.map((role) => (
              <div
                key={role.localId}
                id={`login-role-${role.localId}`}
                className="p-2.5 rounded border border-slate-800 bg-[#020617] space-y-2"
              >
                <div className="flex items-center gap-1.5">
                  <KeyRound className="w-3.5 h-3.5 text-amber-400 shrink-0" />
                  <input
                    type="text"
                    placeholder="Role name (e.g. admin)"
                    value={role.name}
                    onChange={(e) => updateAuthRoleName(role.localId, e.target.value)}
                    disabled={disabled || isBusy}
                    className="flex-1 px-2 py-1 rounded bg-[#0B1120] border border-slate-700 hover:border-slate-600 focus:border-amber-500 text-xs font-mono text-slate-100 placeholder-slate-500 focus:outline-none"
                  />
                  <button
                    type="button"
                    onClick={() => removeAuthRole(role.localId)}
                    disabled={disabled || isBusy}
                    title="Remove this role"
                    className="p-1 rounded text-rose-400 hover:text-rose-300 disabled:opacity-50 cursor-pointer"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>

                {role.mode === 'quick' && role.steps.length === 4 ? (
                  <QuickLoginFields
                    role={role}
                    updateStep={updateStep}
                    onSwitchToAdvanced={() => setRoleMode(role.localId, 'advanced')}
                    disabled={disabled || isBusy}
                  />
                ) : (
                <div className="space-y-1.5 pl-2 border-l border-slate-800">
                  {role.steps.map((step, idx) => {
                    const caption = stepCaption(idx);
                    return (
                    <div key={step.localId} className="space-y-0.5">
                      {caption && (
                        <div className="text-[9px] font-mono text-teal-500/80 uppercase tracking-wide pl-[26px]">
                          {caption}
                        </div>
                      )}
                    <div className="flex items-center gap-1.5">
                      <span className="text-[10px] font-mono text-content-muted w-4 text-right shrink-0">{idx + 1}.</span>
                      <select
                        value={step.type}
                        onChange={(e) =>
                          updateStep(role.localId, step.localId, { type: e.target.value as LoginStepType })
                        }
                        disabled={disabled || isBusy}
                        className="px-1.5 py-1 rounded bg-[#0B1120] border border-slate-700 text-[10px] font-mono text-slate-300 focus:outline-none shrink-0 cursor-pointer"
                      >
                        <option value="NAVIGATE">NAVIGATE</option>
                        <option value="FILL">FILL</option>
                        <option value="CLICK">CLICK</option>
                      </select>

                      {step.type === 'NAVIGATE' && (
                        <input
                          type="text"
                          placeholder="https://your-app.example.com/login"
                          value={step.url}
                          onChange={(e) => updateStep(role.localId, step.localId, { url: e.target.value })}
                          disabled={disabled || isBusy}
                          className="flex-1 px-2 py-1 rounded bg-[#0B1120] border border-slate-700 text-[11px] font-mono text-slate-100 placeholder-slate-500 focus:outline-none"
                        />
                      )}

                      {(step.type === 'FILL' || step.type === 'CLICK') && (
                        <input
                          type="text"
                          placeholder={selectorPlaceholder(caption)}
                          value={step.selector}
                          onChange={(e) => updateStep(role.localId, step.localId, { selector: e.target.value })}
                          disabled={disabled || isBusy}
                          className="w-28 px-2 py-1 rounded bg-[#0B1120] border border-slate-700 text-[11px] font-mono text-slate-100 placeholder-slate-500 focus:outline-none shrink-0"
                        />
                      )}

                      {step.type === 'FILL' && (
                        <div className="flex-1 flex items-center gap-1 min-w-0">
                          <input
                            type={step.isSecret ? 'password' : 'text'}
                            placeholder="value"
                            value={step.value}
                            onChange={(e) => updateStep(role.localId, step.localId, { value: e.target.value })}
                            disabled={disabled || isBusy}
                            className="flex-1 min-w-0 px-2 py-1 rounded bg-[#0B1120] border border-slate-700 text-[11px] font-mono text-slate-100 placeholder-slate-500 focus:outline-none"
                          />
                          <button
                            type="button"
                            onClick={() => updateStep(role.localId, step.localId, { isSecret: !step.isSecret })}
                            title={step.isSecret ? 'Value hidden — click to reveal while typing' : 'Click to mask this value (e.g. a password)'}
                            className="p-1 rounded text-content-muted hover:text-slate-300 shrink-0 cursor-pointer"
                          >
                            {step.isSecret ? <EyeOff className="w-3 h-3" /> : <Eye className="w-3 h-3" />}
                          </button>
                        </div>
                      )}

                      <button
                        type="button"
                        onClick={() => removeStep(role.localId, step.localId)}
                        disabled={disabled || isBusy}
                        title="Remove this step"
                        className="p-1 rounded text-content-muted hover:text-rose-400 disabled:opacity-50 shrink-0 cursor-pointer"
                      >
                        <Trash2 className="w-3 h-3" />
                      </button>
                    </div>
                    </div>
                    );
                  })}
                  <div className="flex items-center gap-3">
                    <button
                      type="button"
                      onClick={() => addStep(role.localId)}
                      disabled={disabled || isBusy}
                      className="flex items-center gap-1 text-[10px] font-mono text-teal-400 hover:text-teal-300 disabled:opacity-50 cursor-pointer"
                    >
                      <Plus className="w-3 h-3" />
                      <span>Add step</span>
                    </button>
                    {role.steps.length === 4 && (
                      <button
                        type="button"
                        onClick={() => setRoleMode(role.localId, 'quick')}
                        disabled={disabled || isBusy}
                        className="text-[10px] font-mono text-content-muted hover:text-slate-300 disabled:opacity-50 cursor-pointer"
                      >
                        ← Back to Quick
                      </button>
                    )}
                  </div>
                </div>
                )}
              </div>
            ))}

            <button
              type="button"
              id="btn-add-login-role"
              onClick={addAuthRole}
              disabled={disabled || isBusy}
              className="flex items-center gap-1.5 text-[11px] font-mono text-amber-400 hover:text-amber-300 disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
            >
              <Plus className="w-3 h-3" />
              <span>Add another role</span>
            </button>

            <p className="text-[10px] text-content-muted font-sans flex items-start gap-1">
              <Info className="w-3 h-3 shrink-0 mt-0.5" />
              <span>
                Login values (including any password) are sent as plain JSON and embedded in plaintext in the
                generated crawl spec — same real, existing exposure any compiled FILL action already has in this app
                (that directory is gitignored, but not further encrypted).
              </span>
            </p>
            <p id="crawl-env-hint" className="text-[10px] text-content-muted font-sans flex items-start gap-1">
              <KeyRound className="w-3 h-3 shrink-0 mt-0.5" />
              <span>
                To log in without a password being stored, write a value as <code>env:AIQA_LOGIN_ADMIN_PASSWORD</code>: it
                is read from the orchestrator's environment when the crawl — and any test compiled from it — runs, and
                nothing is embedded. A role crawled with a literal password cannot be used to compile tests.
              </span>
            </p>
          </div>
        )}
      </div>

      {status === 'COMPLETED' && output && (
        <div className="p-2.5 rounded bg-[#020617] border border-emerald-900/60 space-y-1.5">
          <div className="flex items-center gap-1.5 text-xs font-mono text-emerald-300">
            <CheckCircle2 className="w-3.5 h-3.5 shrink-0" />
            <span>
              Crawled {totalPages} real page{totalPages === 1 ? '' : 's'}, {totalElements} real element
              {totalElements === 1 ? '' : 's'}
              {lastRunId ? ` (run ${lastRunId.slice(0, 8)})` : ''}.
            </span>
          </div>
          <label className="flex items-center gap-1.5 text-[11px] font-mono text-slate-300 cursor-pointer">
            <input
              type="checkbox"
              id="crawl-use-in-generation"
              checked={crawlRunId !== null}
              onChange={(e) => onCrawlRunIdChange(e.target.checked ? lastRunId : null)}
              disabled={disabled}
              className="accent-teal-600"
            />
            <span>Use this real crawled application data when generating</span>
          </label>
        </div>
      )}

      {status === 'FAILED' && error && (
        <div className="p-2 rounded bg-rose-950/40 border border-rose-800/80 text-[11px] font-mono text-rose-300 flex items-center gap-1.5">
          <XCircle className="w-3.5 h-3.5 shrink-0" />
          <span>{error}</span>
        </div>
      )}

      {status === 'IDLE' && error && (
        <div className="p-2 rounded bg-amber-950/40 border border-amber-800/80 text-[11px] font-mono text-amber-300 flex items-center gap-1.5">
          <Info className="w-3.5 h-3.5 shrink-0" />
          <span>{error}</span>
        </div>
      )}
    </div>
  );
};

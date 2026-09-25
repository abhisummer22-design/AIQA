/**
 * Phase 7 — the roles an autopilot run crawls and tests as, each with the login steps a person confirmed.
 *
 * The anonymous visitor is always a role and needs no steps. For every other role a person can write the steps by hand or ask
 * for them to be proposed (`POST .../login-suggestion`, `login_suggest.rs`): the page of a crawl that has a password field is
 * read, and the steps that log the role in are proposed with how sure they are and why — by rules first, and by the router's AI
 * when the rules are not sure and the person allows it. Nothing is applied until the person takes it, and a value in a step is an
 * `env:NAME` reference to a variable set in the orchestrator's environment, never a credential typed here.
 *
 * With no crawl of the application yet there is nothing to propose from: the editor says so and offers to look at the login page
 * (an anonymous crawl, the same one the Test Case Generator makes) and then proposes.
 */
import React, { useEffect, useRef, useState } from 'react';
import { KeyRound, Loader2, Plus, Sparkles, Trash2, Wand2 } from 'lucide-react';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import type { LoginSuggestion, LoginSuggestionResponse, PlanAction } from '../../types/contract';
import {
  ANONYMOUS_ROLE,
  MAX_ROLES,
  isHttpAddress,
  roleProblem,
  stepsFromActions,
  type LoginStepDraft,
  type LoginStepType,
  type RoleDraft,
} from './autopilotModel';

const POLL_MS = 2_000;
const MAX_POLLS = 45;
const ENDED = new Set(['COMPLETED', 'PARTIAL', 'FAILED', 'CANCELLED']);

type SuggestionState =
  | { status: 'loading' }
  | { status: 'crawling' }
  | { status: 'done'; response: LoginSuggestionResponse }
  | { status: 'no-crawl'; message: string }
  | { status: 'error'; message: string };

const CONFIDENCE_CLS: Record<LoginSuggestion['confidence'], string> = {
  high: 'text-emerald-300 bg-emerald-950/50 border-emerald-800/70',
  medium: 'text-amber-300 bg-amber-950/50 border-amber-800/70',
  low: 'text-rose-300 bg-rose-950/50 border-rose-800/70',
};

const INPUT_CLS = 'rounded-md border border-slate-800 bg-[#020617] px-2 py-1 text-xs text-slate-200 outline-none focus:ring-1 focus:ring-violet-500 disabled:opacity-50';

/** A step as one short line. */
export function describeAction(action: PlanAction): string {
  switch (action.type) {
    case 'NAVIGATE':
      return `open ${action.url}`;
    case 'CLICK':
      return `click ${action.selector}`;
    case 'FILL':
      return `fill ${action.selector} with ${action.value}`;
    default:
      return action.type;
  }
}

const SuggestionCard: React.FC<{ id: string; suggestion: LoginSuggestion; primary: boolean; onUse: () => void; disabled?: boolean }> = ({ id, suggestion, primary, onUse, disabled }) => (
  <div id={id} className="p-2 rounded bg-[#020617] border border-slate-800 space-y-1.5">
    <div className="flex flex-wrap items-center gap-2">
      <span className={`px-1.5 py-0.5 rounded border text-[10px] font-mono uppercase ${CONFIDENCE_CLS[suggestion.confidence]}`}>{suggestion.confidence}</span>
      <span className="text-[10px] text-slate-500">
        {primary ? 'proposed' : 'another answer'} by {suggestion.by} from <span className="font-mono">{suggestion.page_url}</span>
      </span>
    </div>
    <ol className="list-decimal pl-5 space-y-0.5 text-[11px] font-mono text-slate-200">
      {suggestion.actions.map((a, i) => (
        <li key={i}>{describeAction(a)}</li>
      ))}
    </ol>
    {suggestion.reasons.length > 0 && (
      <ul className="list-disc pl-5 text-[10px] text-slate-400 space-y-0.5">
        {suggestion.reasons.map((r) => (
          <li key={r}>{r}</li>
        ))}
      </ul>
    )}
    <button
      type="button"
      onClick={onUse}
      disabled={disabled}
      className="px-2 py-1 rounded border border-violet-700 bg-violet-950/50 text-[11px] text-violet-200 hover:bg-violet-900/60 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
    >
      Use these steps
    </button>
  </div>
);

interface Props {
  projectId: string | null;
  baseUrl: string;
  roles: RoleDraft[];
  onChange: (roles: RoleDraft[]) => void;
  /** A fresh id for a role or a step. */
  nextId: () => number;
  disabled?: boolean;
}

export const LoginRolesEditor: React.FC<Props> = ({ projectId, baseUrl, roles, onChange, nextId, disabled }) => {
  const [suggestions, setSuggestions] = useState<Record<number, SuggestionState | undefined>>({});
  const [useAi, setUseAi] = useState(true);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const setSuggestion = (localId: number, state: SuggestionState | undefined) => {
    if (alive.current) setSuggestions((prev) => ({ ...prev, [localId]: state }));
  };

  const updateRole = (localId: number, patch: Partial<RoleDraft>) => onChange(roles.map((r) => (r.localId === localId ? { ...r, ...patch } : r)));

  const updateStep = (role: RoleDraft, stepId: number, patch: Partial<LoginStepDraft>) =>
    updateRole(role.localId, { steps: role.steps.map((s) => (s.localId === stepId ? { ...s, ...patch } : s)) });

  const addStep = (role: RoleDraft, type: LoginStepType) =>
    updateRole(role.localId, { steps: [...role.steps, { localId: nextId(), type, url: type === 'NAVIGATE' ? baseUrl.trim() : '', selector: '', value: '' }] });

  const suggest = async (role: RoleDraft) => {
    if (!projectId) return;
    setSuggestion(role.localId, { status: 'loading' });
    try {
      const response = await aiqaApi.loginSuggestion.suggest(projectId, { role: role.name.trim(), use_ai: useAi });
      setSuggestion(role.localId, { status: 'done', response });
    } catch (e) {
      if (e instanceof ApiError && (e.kind === 'not_found' || e.status === 404)) {
        setSuggestion(role.localId, { status: 'no-crawl', message: e.detail ?? e.message });
      } else if (e instanceof ApiError && e.kind === 'validation_error' && /password|login page/i.test(e.detail ?? '')) {
        setSuggestion(role.localId, { status: 'no-crawl', message: e.detail ?? e.message });
      } else {
        setSuggestion(role.localId, { status: 'error', message: e instanceof ApiError ? e.detail ?? e.message : 'The suggestion could not be made.' });
      }
    }
  };

  /** An anonymous crawl of the application, waited for, then the suggestion asked for again. */
  const lookAtLoginPage = async (role: RoleDraft) => {
    if (!projectId) return;
    setSuggestion(role.localId, { status: 'crawling' });
    try {
      const started = await aiqaApi.crawl.start(projectId, { base_url: baseUrl.trim(), roles: [{ name: ANONYMOUS_ROLE, login_actions: [] }] });
      for (let i = 0; i < MAX_POLLS; i++) {
        await new Promise((resolve) => setTimeout(resolve, POLL_MS));
        if (!alive.current) return;
        const run = await aiqaApi.executions.get(projectId, started.run_id);
        if (ENDED.has(run.state)) {
          if (run.state !== 'COMPLETED') {
            const reason = run.tasks?.[0]?.reason;
            setSuggestion(role.localId, { status: 'error', message: `The crawl of the login page ended ${run.state}${reason ? `: ${reason}` : ''}.` });
            return;
          }
          await suggest(role);
          return;
        }
      }
      setSuggestion(role.localId, { status: 'error', message: 'The crawl of the login page did not finish in time; try again in a moment.' });
    } catch (e) {
      setSuggestion(role.localId, { status: 'error', message: e instanceof ApiError ? e.detail ?? e.message : 'The login page could not be crawled.' });
    }
  };

  const takeSuggestion = (role: RoleDraft, suggestion: LoginSuggestion) => {
    updateRole(role.localId, { steps: stepsFromActions(suggestion.actions, nextId) });
    setSuggestion(role.localId, undefined);
  };

  const canAdd = roles.length + 1 < MAX_ROLES;

  return (
    <div id="autopilot-roles" className="space-y-2">
      <div className="flex flex-wrap items-center gap-3 text-[11px] text-slate-400">
        <span className="inline-flex items-center gap-1.5">
          <KeyRound className="w-3.5 h-3.5 text-violet-400" />
          <span className="font-mono text-slate-200">{ANONYMOUS_ROLE}</span> is always crawled: the visitor who has not logged in.
        </span>
        <label className="inline-flex items-center gap-1.5 cursor-pointer">
          <input id="roles-use-ai" type="checkbox" checked={useAi} onChange={(e) => setUseAi(e.target.checked)} className="accent-violet-500" disabled={disabled} />
          when suggesting steps, also ask the AI if the rules are not sure
        </label>
      </div>

      {roles.map((role, index) => {
        const problem = roleProblem(role);
        const suggestion = suggestions[role.localId];
        return (
          <div key={role.localId} id={`role-${index}`} className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2">
            <div className="flex flex-wrap items-center gap-2">
              <input
                id={`role-${index}-name`}
                aria-label={`Role ${index + 1} name`}
                value={role.name}
                onChange={(e) => updateRole(role.localId, { name: e.target.value })}
                placeholder="Role name (admin, customer…)"
                disabled={disabled}
                className={`${INPUT_CLS} w-52`}
              />
              <button
                type="button"
                id={`role-${index}-suggest`}
                onClick={() => void suggest(role)}
                disabled={disabled || !projectId || !role.name.trim() || suggestion?.status === 'loading' || suggestion?.status === 'crawling'}
                className="inline-flex items-center gap-1 px-2 py-1 rounded border border-violet-800 text-[11px] text-violet-200 hover:bg-violet-950/50 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Wand2 className="w-3.5 h-3.5" /> Suggest login steps
              </button>
              <button
                type="button"
                id={`role-${index}-remove`}
                aria-label={`Remove role ${index + 1}`}
                onClick={() => onChange(roles.filter((r) => r.localId !== role.localId))}
                disabled={disabled}
                className="ml-auto p-1 rounded text-slate-500 hover:text-rose-300 hover:bg-slate-800/60 cursor-pointer disabled:opacity-50"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            </div>

            {(suggestion?.status === 'loading' || suggestion?.status === 'crawling') && (
              <p id={`role-${index}-busy`} className="flex items-center gap-1.5 text-[11px] text-violet-300">
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                {suggestion.status === 'crawling' ? 'Looking at the login page (a crawl as the visitor)…' : 'Reading the login page…'}
              </p>
            )}
            {suggestion?.status === 'no-crawl' && (
              <div id={`role-${index}-no-crawl`} className="p-2 rounded bg-amber-950/20 border border-amber-800/50 text-[11px] text-amber-200 space-y-1.5">
                <p>{suggestion.message}</p>
                <button
                  type="button"
                  id={`role-${index}-crawl`}
                  onClick={() => void lookAtLoginPage(role)}
                  disabled={disabled || !isHttpAddress(baseUrl)}
                  className="px-2 py-1 rounded border border-amber-700 text-[11px] hover:bg-amber-950/50 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Look at the login page (crawl as the visitor)
                </button>
                {!isHttpAddress(baseUrl) && <p className="text-[10px] text-amber-400/80">Give the application’s address first.</p>}
              </div>
            )}
            {suggestion?.status === 'error' && (
              <p id={`role-${index}-error`} role="alert" className="text-[11px] text-rose-300">
                {suggestion.message}
              </p>
            )}
            {suggestion?.status === 'done' && (
              <div id={`role-${index}-suggestion`} className="space-y-1.5">
                <SuggestionCard id={`role-${index}-suggestion-0`} suggestion={suggestion.response.suggestion} primary onUse={() => takeSuggestion(role, suggestion.response.suggestion)} disabled={disabled} />
                {suggestion.response.alternatives.map((alt, i) => (
                  <SuggestionCard key={i} id={`role-${index}-suggestion-${i + 1}`} suggestion={alt} primary={false} onUse={() => takeSuggestion(role, alt)} disabled={disabled} />
                ))}
                {suggestion.response.ai.requested && !suggestion.response.ai.used && suggestion.response.ai.note && (
                  <p className="text-[10px] text-slate-500">AI: {suggestion.response.ai.note}</p>
                )}
                {suggestion.response.missing_secrets.length > 0 && (
                  <p id={`role-${index}-missing-secrets`} className="text-[11px] text-amber-300">
                    <Sparkles className="inline w-3 h-3 mr-1" />
                    Not set in the orchestrator’s environment yet: <span className="font-mono">{suggestion.response.missing_secrets.join(', ')}</span>. Set them before starting — a run that
                    logs in with them is refused until they exist. Never type the real values here.
                  </p>
                )}
              </div>
            )}

            <ol className="space-y-1">
              {role.steps.map((step, i) => (
                <li key={step.localId} id={`role-${index}-step-${i}`} className="flex flex-wrap items-center gap-1.5">
                  <span className="text-[10px] text-slate-500 w-4">{i + 1}.</span>
                  <select
                    aria-label={`Role ${index + 1} step ${i + 1} type`}
                    value={step.type}
                    onChange={(e) => updateStep(role, step.localId, { type: e.target.value as LoginStepType })}
                    disabled={disabled}
                    className={INPUT_CLS}
                  >
                    <option value="NAVIGATE">Open page</option>
                    <option value="FILL">Fill field</option>
                    <option value="CLICK">Click</option>
                  </select>
                  {step.type === 'NAVIGATE' && (
                    <input
                      aria-label={`Role ${index + 1} step ${i + 1} address`}
                      value={step.url}
                      onChange={(e) => updateStep(role, step.localId, { url: e.target.value })}
                      placeholder="https://…/login"
                      disabled={disabled}
                      className={`${INPUT_CLS} w-72`}
                    />
                  )}
                  {step.type !== 'NAVIGATE' && (
                    <input
                      aria-label={`Role ${index + 1} step ${i + 1} selector`}
                      value={step.selector}
                      onChange={(e) => updateStep(role, step.localId, { selector: e.target.value })}
                      placeholder="CSS selector"
                      disabled={disabled}
                      className={`${INPUT_CLS} w-56 font-mono`}
                    />
                  )}
                  {step.type === 'FILL' && (
                    <input
                      aria-label={`Role ${index + 1} step ${i + 1} value`}
                      value={step.value}
                      onChange={(e) => updateStep(role, step.localId, { value: e.target.value })}
                      placeholder="env:AIQA_LOGIN_ROLE_PASSWORD"
                      disabled={disabled}
                      className={`${INPUT_CLS} w-64 font-mono`}
                    />
                  )}
                  <button
                    type="button"
                    aria-label={`Remove step ${i + 1} of role ${index + 1}`}
                    onClick={() => updateRole(role.localId, { steps: role.steps.filter((s) => s.localId !== step.localId) })}
                    disabled={disabled}
                    className="p-1 rounded text-slate-500 hover:text-rose-300 cursor-pointer disabled:opacity-50"
                  >
                    <Trash2 className="w-3 h-3" />
                  </button>
                </li>
              ))}
            </ol>
            <div className="flex flex-wrap items-center gap-2">
              {(['NAVIGATE', 'FILL', 'CLICK'] as LoginStepType[]).map((type) => (
                <button
                  key={type}
                  type="button"
                  id={`role-${index}-add-${type.toLowerCase()}`}
                  onClick={() => addStep(role, type)}
                  disabled={disabled}
                  className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded border border-slate-700 text-[10px] text-slate-300 hover:bg-slate-800/60 cursor-pointer disabled:opacity-50"
                >
                  <Plus className="w-3 h-3" /> {type === 'NAVIGATE' ? 'open page' : type === 'FILL' ? 'fill field' : 'click'}
                </button>
              ))}
              <span className="text-[10px] text-slate-500">
                Use <span className="font-mono">env:AIQA_LOGIN_…</span> for user names and passwords: the value is read from the orchestrator’s environment, never stored.
              </span>
            </div>
            {problem && (
              <p id={`role-${index}-problem`} className="text-[11px] text-rose-300">
                {problem}
              </p>
            )}
          </div>
        );
      })}

      <button
        type="button"
        id="roles-add"
        onClick={() => onChange([...roles, { localId: nextId(), name: '', steps: [] }])}
        disabled={disabled || !canAdd}
        className="inline-flex items-center gap-1 px-2 py-1 rounded border border-slate-700 text-[11px] text-slate-200 hover:bg-slate-800/60 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <Plus className="w-3.5 h-3.5" /> Add a role that logs in
      </button>
    </div>
  );
};

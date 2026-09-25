import React, { useState } from 'react';
import { ArrowDown, ArrowUp, ChevronDown, ChevronRight, Plus, Trash2 } from 'lucide-react';
import type { AiProviderInfo, TaskProviderPreference, TaskTarget } from '../../types/contract';

/** The tasks the router really serves today — each has a real caller. */
const CHAIN_TASKS: ReadonlyArray<{ task: string; label: string }> = [
  { task: 'TEST_CASE_GENERATION', label: 'Test case generation' },
  { task: 'REQUIREMENT_ANALYSIS', label: 'Requirement analysis' },
  { task: 'TEST_CASE_COMPILATION', label: 'Test case compilation' },
  { task: 'BUG_ANALYSIS', label: 'Bug analysis' },
  // Phase 6: the AI that checks the requirements the gap check's words left unclear (`gap_verify.rs`).
  { task: 'TRACEABILITY', label: 'Requirement gap check' },
  // Phase 7: the AI that proposes a role's login steps when the rules are not sure (`login_suggest.rs`); plain text, so a small local
  // model will do.
  { task: 'GENERAL_AI', label: 'Login suggestion' },
];

type TaskOverrides = Record<string, TaskProviderPreference>;

const SELECT_CLS =
  'px-2 py-1 rounded bg-surface-card border border-slate-700 text-[11px] font-mono text-content-primary focus:outline-none focus:border-violet-500 cursor-pointer disabled:opacity-50';
const ICON_BTN_CLS =
  'p-1 rounded text-content-muted hover:bg-slate-800/60 cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed';

/** A model that can generate text at all — an embedding or audio model can never serve these tasks. */
const usableModels = (p: AiProviderInfo) => p.models.filter((m) => m.capabilities.text_generation !== 'NONE');

interface TaskChainProps {
  task: string;
  label: string;
  providers: AiProviderInfo[];
  chain: TaskTarget[];
  onChange: (chain: TaskTarget[]) => void;
}

const TaskChain: React.FC<TaskChainProps> = ({ task, label, providers, chain, onChange }) => {
  const [provider, setProvider] = useState('');
  const [model, setModel] = useState('');
  const choices = providers.filter((p) => usableModels(p).length > 0);
  const chosen = choices.find((p) => p.id === provider);
  const alreadyListed = chain.some((t) => t.provider === provider && t.model === model);

  const move = (from: number, to: number) => {
    const next = [...chain];
    [next[from], next[to]] = [next[to], next[from]];
    onChange(next);
  };

  return (
    <div id={`task-chain-${task}`} className="p-2 rounded-md bg-surface-card border border-border-default space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <span className="font-mono text-xs font-semibold text-content-primary">{label}</span>
        {chain.length === 0 && <span className="text-[10px] font-mono text-content-muted">Automatic</span>}
      </div>

      {chain.length > 0 && (
        <ol className="space-y-1">
          {chain.map((t, i) => {
            const p = providers.find((x) => x.id === t.provider);
            const m = p?.models.find((x) => x.id === t.model);
            return (
              <li key={`${i}-${t.provider}/${t.model}`} className="flex items-center gap-2 text-[11px] font-mono">
                <span className="w-4 text-content-muted">{i + 1}.</span>
                <span className="text-content-primary truncate">
                  {p?.name ?? t.provider} / {m?.display_name ?? t.model}
                </span>
                <span className="text-[10px] text-content-muted">{i === 0 ? 'primary' : 'fallback'}</span>
                {p?.pricing.kind === 'METERED' && (
                  <span
                    className="text-[10px] px-1.5 rounded bg-amber-950/40 border border-amber-800/60 text-amber-300"
                    title={`$${p.pricing.input_usd_per_1k}/1K tokens in · $${p.pricing.output_usd_per_1k}/1K tokens out`}
                  >
                    paid
                  </span>
                )}
                {!m && <span className="text-[10px] text-amber-300">not available right now — skipped</span>}
                <span className="ml-auto flex items-center shrink-0">
                  <button
                    type="button"
                    aria-label={`Move ${t.model} up in ${label}`}
                    title="Move up"
                    onClick={() => move(i, i - 1)}
                    disabled={i === 0}
                    className={`${ICON_BTN_CLS} hover:text-violet-300`}
                  >
                    <ArrowUp className="w-3 h-3" />
                  </button>
                  <button
                    type="button"
                    aria-label={`Move ${t.model} down in ${label}`}
                    title="Move down"
                    onClick={() => move(i, i + 1)}
                    disabled={i === chain.length - 1}
                    className={`${ICON_BTN_CLS} hover:text-violet-300`}
                  >
                    <ArrowDown className="w-3 h-3" />
                  </button>
                  <button
                    type="button"
                    aria-label={`Remove ${t.model} from ${label}`}
                    title="Remove from this list"
                    onClick={() => onChange(chain.filter((_, j) => j !== i))}
                    className={`${ICON_BTN_CLS} hover:text-rose-300`}
                  >
                    <Trash2 className="w-3 h-3" />
                  </button>
                </span>
              </li>
            );
          })}
        </ol>
      )}

      <div className="flex flex-wrap items-center gap-1.5">
        <select
          id={`task-chain-${task}-provider`}
          aria-label={`${label}: provider to add`}
          value={provider}
          onChange={(e) => {
            setProvider(e.target.value);
            setModel('');
          }}
          className={SELECT_CLS}
        >
          <option value="">Provider…</option>
          {choices.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <select
          id={`task-chain-${task}-model`}
          aria-label={`${label}: model to add`}
          value={model}
          onChange={(e) => setModel(e.target.value)}
          disabled={!chosen}
          className={SELECT_CLS}
        >
          <option value="">Model…</option>
          {chosen &&
            usableModels(chosen).map((m) => (
              <option key={m.id} value={m.id}>
                {m.display_name}
              </option>
            ))}
        </select>
        <button
          type="button"
          id={`task-chain-${task}-add`}
          onClick={() => {
            onChange([...chain, { provider, model }]);
            setModel('');
          }}
          disabled={!provider || !model || alreadyListed}
          className="flex items-center gap-1 px-2 py-1 rounded bg-violet-600 hover:bg-violet-500 text-white font-mono text-[11px] font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <Plus className="w-3 h-3" />
          <span>Add</span>
        </button>
      </div>
    </div>
  );
};

interface TaskRoutingChainsProps {
  providers: AiProviderInfo[];
  overrides: TaskOverrides;
  onChange: (next: TaskOverrides) => void;
}

/** Per-task ordered lists of exact provider/model pairs (`task_overrides[task].chain`). A task with a list uses
 *  only those models, in that order; a task without one keeps the usual automatic routing. Other fields of an
 *  override (a pinned provider/model) are left exactly as they were. */
export const TaskRoutingChains: React.FC<TaskRoutingChainsProps> = ({ providers, overrides, onChange }) => {
  const [open, setOpen] = useState(() => CHAIN_TASKS.some(({ task }) => (overrides[task]?.chain?.length ?? 0) > 0));

  const setChain = (task: string, chain: TaskTarget[]) => {
    const next = { ...overrides };
    const updated: TaskProviderPreference = { provider: null, model: null, ...overrides[task], chain };
    if (chain.length === 0) delete updated.chain;
    if (chain.length === 0 && !updated.provider && !updated.model) delete next[task];
    else next[task] = updated;
    onChange(next);
  };

  return (
    <div id="task-routing-chains" className="pt-2 border-t border-border-default space-y-2">
      <button
        type="button"
        id="btn-toggle-task-routing"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-1.5 text-xs font-bold font-mono text-content-secondary hover:text-violet-300 cursor-pointer"
      >
        {open ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
        <span>Task-wise routing (optional)</span>
      </button>

      {open && (
        <div className="space-y-2">
          <p className="text-[10px] font-mono text-content-muted">
            Choose exactly which models a task may use, in order. The first is tried first, the next ones only if it
            fails. A model that is not in the list is never used for that task, so a paid model runs only where you
            list it. A task with no list keeps the automatic routing.
          </p>
          {CHAIN_TASKS.map(({ task, label }) => (
            <TaskChain
              key={task}
              task={task}
              label={label}
              providers={providers}
              chain={overrides[task]?.chain ?? []}
              onChange={(chain) => setChain(task, chain)}
            />
          ))}
        </div>
      )}
    </div>
  );
};

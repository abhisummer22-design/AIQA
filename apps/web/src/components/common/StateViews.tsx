/**
 * The six honest UI states (audit brief PART 11).
 *
 * Each of these is a distinct thing the user needs to be able to tell apart:
 *
 *   Loading         a request is genuinely in flight
 *   Empty           the backend answered successfully with no rows
 *   Error           the request failed
 *   Unauthorized    authentication failed or is missing
 *   Offline         the orchestrator could not be reached at all
 *   NotImplemented  the backend has no such capability — the UI says so
 *                   instead of inventing data to fill the space
 *
 * Nothing in this file renders sample rows, skeleton "example" content, or
 * placeholder numbers. An empty table is shown as empty.
 */

import React from "react";
import {
  AlertTriangle,
  Ban,
  Inbox,
  KeyRound,
  Loader2,
  PlugZap,
  RefreshCw,
} from "lucide-react";
import { ApiError } from "../../services/apiClient";

// ------------------------------------------------------------------ shell

interface ShellProps {
  icon: React.ReactNode;
  title: string;
  message?: string;
  tone?: "neutral" | "warn" | "danger" | "info";
  action?: { label: string; onClick: () => void };
  compact?: boolean;
}

const TONE_CLASSES: Record<NonNullable<ShellProps["tone"]>, string> = {
  neutral: "text-slate-400 border-slate-700/60 bg-slate-900/40",
  info: "text-sky-300 border-sky-800/60 bg-sky-950/30",
  warn: "text-amber-300 border-amber-800/60 bg-amber-950/30",
  danger: "text-rose-300 border-rose-800/60 bg-rose-950/30",
};

function StateShell({ icon, title, message, tone = "neutral", action, compact = false }: ShellProps) {
  return (
    <div
      role="status"
      className={`flex flex-col items-center justify-center gap-2 rounded-xl border text-center ${
        TONE_CLASSES[tone]
      } ${compact ? "px-4 py-5" : "px-6 py-10"}`}
    >
      <div className="opacity-80">{icon}</div>
      <div className="text-sm font-semibold tracking-tight">{title}</div>
      {message && <p className="max-w-md text-xs leading-relaxed opacity-70">{message}</p>}
      {action && (
        <button
          onClick={action.onClick}
          className="mt-2 inline-flex items-center gap-1.5 rounded-md border border-current/30 px-3 py-1.5 text-xs font-medium transition hover:bg-white/5"
        >
          <RefreshCw size={12} />
          {action.label}
        </button>
      )}
    </div>
  );
}

// ------------------------------------------------------------------ states

export function LoadingState({ label = "Loading from orchestrator…", compact }: { label?: string; compact?: boolean }) {
  return (
    <StateShell
      icon={<Loader2 size={compact ? 16 : 22} className="animate-spin" />}
      title={label}
      tone="neutral"
      compact={compact}
    />
  );
}

export function EmptyState({
  title = "No data yet",
  message,
  compact,
}: {
  title?: string;
  message?: string;
  compact?: boolean;
}) {
  return (
    <StateShell
      icon={<Inbox size={compact ? 16 : 22} />}
      title={title}
      message={message ?? "The orchestrator responded successfully, but has no records to show."}
      tone="neutral"
      compact={compact}
    />
  );
}

export function OfflineState({ onRetry, compact }: { onRetry?: () => void; compact?: boolean }) {
  return (
    <StateShell
      icon={<PlugZap size={compact ? 16 : 22} />}
      title="Orchestrator unreachable"
      message="No response from the AIQA orchestrator. Start it with: cargo run -p aiqa-orchestrator (listens on port 8080)."
      tone="danger"
      action={onRetry ? { label: "Retry", onClick: onRetry } : undefined}
      compact={compact}
    />
  );
}

export function UnauthorizedState({
  kind,
  onRetry,
  compact,
}: {
  kind: "unauthenticated" | "unauthorized" | "forbidden";
  onRetry?: () => void;
  compact?: boolean;
}) {
  const copy = {
    unauthenticated: {
      title: "Authentication required",
      message:
        "Enter an orchestrator API key in Settings. The orchestrator prints a bootstrap admin key to stderr on first start.",
    },
    unauthorized: {
      title: "API key rejected",
      message: "The stored key is invalid or has been revoked. Enter a current key in Settings.",
    },
    forbidden: {
      title: "Not permitted",
      message: "This key's role or project scope does not allow this action. An admin key is required for admin routes.",
    },
  }[kind];

  return (
    <StateShell
      icon={kind === "forbidden" ? <Ban size={compact ? 16 : 22} /> : <KeyRound size={compact ? 16 : 22} />}
      title={copy.title}
      message={copy.message}
      tone="warn"
      action={onRetry ? { label: "Retry", onClick: onRetry } : undefined}
      compact={compact}
    />
  );
}

export function ErrorState({ error, onRetry, compact }: { error: ApiError; onRetry?: () => void; compact?: boolean }) {
  return (
    <StateShell
      icon={<AlertTriangle size={compact ? 16 : 22} />}
      title={error.status ? `Request failed (HTTP ${error.status})` : "Request failed"}
      message={error.detail || error.message}
      tone="danger"
      action={onRetry && error.isRetryable ? { label: "Retry", onClick: onRetry } : undefined}
      compact={compact}
    />
  );
}

/**
 * The state that replaces fabricated screens. Names the missing backend
 * capability explicitly rather than showing "Coming Soon" — an operator
 * should be able to read this and know exactly what would have to be built.
 */
export function NotImplementedState({
  feature,
  requires,
  compact,
}: {
  feature: string;
  requires: string;
  compact?: boolean;
}) {
  return (
    <StateShell
      icon={<Ban size={compact ? 16 : 22} />}
      title={`${feature} — not implemented`}
      message={`No backend capability exists for this yet. Requires: ${requires}. This screen intentionally shows nothing rather than sample data.`}
      tone="info"
      compact={compact}
    />
  );
}

// ------------------------------------------------------- resource switcher

/**
 * Renders the correct state for a `ResourceState` and only calls `children`
 * when there is genuine data. Components using this cannot accidentally
 * render an error or empty response as if it were content.
 */
export function ResourceView<T>({
  state,
  children,
  emptyTitle,
  emptyMessage,
  loadingLabel,
  compact,
}: {
  state: {
    status: "loading" | "ready" | "empty" | "error";
    data: T | null;
    error: ApiError | null;
    reload: () => void;
  };
  children: (data: T) => React.ReactNode;
  emptyTitle?: string;
  emptyMessage?: string;
  loadingLabel?: string;
  compact?: boolean;
}) {
  if (state.status === "loading") return <LoadingState label={loadingLabel} compact={compact} />;

  if (state.status === "error" && state.error) {
    const { kind } = state.error;
    if (kind === "offline") return <OfflineState onRetry={state.reload} compact={compact} />;
    if (kind === "unauthenticated" || kind === "unauthorized" || kind === "forbidden") {
      return <UnauthorizedState kind={kind} onRetry={state.reload} compact={compact} />;
    }
    return <ErrorState error={state.error} onRetry={state.reload} compact={compact} />;
  }

  if (state.status === "empty") return <EmptyState title={emptyTitle} message={emptyMessage} compact={compact} />;

  return state.data !== null ? <>{children(state.data)}</> : null;
}

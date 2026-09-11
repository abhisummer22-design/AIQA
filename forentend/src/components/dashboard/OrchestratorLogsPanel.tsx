import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Activity,
  AlertCircle,
  ArrowDown,
  Clock,
  Filter,
  Loader2,
  Radio,
  RefreshCw,
  Terminal,
  Wifi,
  WifiOff,
  XCircle,
} from 'lucide-react';
import { aiqaApi, orchestratorLogsWsUrl } from '../../services/aiqaApi';
import type { OrchestratorLogEntry } from '../../types/contract';
import { useAuth } from '../../context/AuthContext';

export interface OrchestratorLogsPanelProps {
  projectId?: string | null;
  className?: string;
}

type ConnectionStatus = 'live' | 'connecting' | 'reconnecting' | 'offline';
type LogLevelFilter = 'ALL' | 'INFO' | 'WARN' | 'ERROR' | 'SUCCESS';

/** Structural guard for anything arriving on this panel's two real inputs
 *  (`GET /api/v1/logs` history and the `/logs/ws` frames). Mirrors the
 *  established pattern in `RealtimeContext.tsx` (`isRunEventShape`): a
 *  payload that is not actually an `OrchestratorLogEntry` is dropped, never
 *  rendered on faith.
 *
 *  This is not defensive decoration — before it existed, a single entry
 *  missing `level` threw inside `levelBadge` during render, and because this
 *  panel now sits inside the Dashboard tree, that unmounted the *entire*
 *  Dashboard rather than just the log list. Dropping an unreadable entry is
 *  the honest failure mode: it is not shown, and nothing else breaks. */
function isLogEntryShape(value: unknown): value is OrchestratorLogEntry {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.id === 'string' &&
    typeof v.timestamp === 'string' &&
    typeof v.level === 'string' &&
    typeof v.source === 'string' &&
    typeof v.message === 'string'
  );
}

function formatTimeOnly(iso: string): string {
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso.slice(11, 19) || iso;
    return d.toTimeString().slice(0, 8);
  } catch {
    return iso;
  }
}

export const OrchestratorLogsPanel: React.FC<OrchestratorLogsPanelProps> = ({ projectId, className = '' }) => {
  const auth = useAuth();
  const [logs, setLogs] = useState<OrchestratorLogEntry[]>([]);
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>('connecting');
  const [loadingInitial, setLoadingInitial] = useState<boolean>(true);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [levelFilter, setLevelFilter] = useState<LogLevelFilter>('ALL');
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [hasUnread, setHasUnread] = useState<boolean>(false);

  const containerRef = useRef<HTMLDivElement>(null);
  const isNearBottomRef = useRef<boolean>(true);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<NodeJS.Timeout | null>(null);
  const retryCountRef = useRef<number>(0);
  const abortControllerRef = useRef<AbortController | null>(null);
  const isUnmountedRef = useRef<boolean>(false);
  /** Retires a socket this component is deliberately replacing (project
   *  change, unmount, explicit reconnect).
   *
   *  Detaching the handlers *before* closing is the whole point. `close()`
   *  fires `onclose` asynchronously, so a component-level "this close was
   *  intentional" flag is already stale by the time it runs — the superseded
   *  socket's `onclose` lands after the replacement socket exists and gets
   *  mistaken for a real drop, which flipped the panel to OFFLINE and
   *  scheduled a pointless reconnect while the new socket was streaming fine.
   *  A detached socket simply cannot report anything, so intent is tracked
   *  where it actually belongs: on the individual socket. */
  const retireSocket = useCallback((ws: WebSocket | null) => {
    if (!ws) return;
    ws.onopen = null;
    ws.onmessage = null;
    ws.onerror = null;
    ws.onclose = null;
    try {
      ws.close();
    } catch {
      // ignore — already closing/closed
    }
  }, []);

  const scrollToBottom = useCallback(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
      setHasUnread(false);
    }
  }, []);

  const handleScroll = useCallback(() => {
    if (!containerRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = containerRef.current;
    const isBottom = scrollHeight - scrollTop - clientHeight < 60;
    isNearBottomRef.current = isBottom;
    if (isBottom) {
      setHasUnread(false);
    }
  }, []);

  // Fetch initial history from real backend
  const fetchRecentLogs = useCallback(async () => {
    abortControllerRef.current?.abort();
    const controller = new AbortController();
    abortControllerRef.current = controller;

    setLoadingInitial(true);
    setErrorMessage(null);

    try {
      const history = await aiqaApi.logs.list({ projectId, limit: 150 }, controller.signal);
      if (controller.signal.aborted) return;
      // Only render entries that are genuinely log-shaped — see
      // `isLogEntryShape`. Anything else is dropped rather than trusted.
      setLogs(Array.isArray(history) ? history.filter(isLogEntryShape) : []);
      // After loading, scroll to bottom if near bottom
      setTimeout(() => {
        if (isNearBottomRef.current && containerRef.current) {
          containerRef.current.scrollTop = containerRef.current.scrollHeight;
        }
      }, 50);
    } catch (err: any) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      if (!isUnmountedRef.current) {
        setErrorMessage(err?.message || 'Failed to load initial orchestrator logs');
      }
    } finally {
      if (!controller.signal.aborted && !isUnmountedRef.current) {
        setLoadingInitial(false);
      }
    }
  }, [projectId]);

  // Connect live WebSocket stream
  const connectWs = useCallback(() => {
    if (auth.status !== 'authenticated') {
      setConnectionStatus('offline');
      return;
    }

    if (wsRef.current) {
      retireSocket(wsRef.current);
      wsRef.current = null;
    }

    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }

    const wsUrl = orchestratorLogsWsUrl(projectId);
    if (!wsUrl) {
      setConnectionStatus('offline');
      return;
    }

    setConnectionStatus((prev) => (prev === 'offline' ? 'connecting' : 'reconnecting'));

    try {
      const ws = new WebSocket(wsUrl);
      wsRef.current = ws;

      ws.onopen = () => {
        if (isUnmountedRef.current) return;
        setConnectionStatus('live');
        setErrorMessage(null);
        retryCountRef.current = 0;
      };

      ws.onmessage = (event) => {
        if (isUnmountedRef.current) return;
        try {
          const parsed: unknown = JSON.parse(event.data);
          if (!isLogEntryShape(parsed)) return; // malformed frame — drop, never render
          const entry: OrchestratorLogEntry = parsed;
          setLogs((prev) => {
            // Deduplicate by id if already present
            if (prev.some((l) => l.id === entry.id)) return prev;
            const updated = [...prev, entry];
            // Bound to maximum 300 entries in browser memory
            return updated.length > 300 ? updated.slice(updated.length - 300) : updated;
          });

          // Auto-scroll if near bottom
          if (isNearBottomRef.current) {
            setTimeout(() => {
              if (containerRef.current) {
                containerRef.current.scrollTop = containerRef.current.scrollHeight;
              }
            }, 20);
          } else {
            setHasUnread(true);
          }
        } catch {
          // ignore malformed frame
        }
      };

      ws.onerror = () => {
        if (isUnmountedRef.current) return;
        setConnectionStatus('offline');
      };

      ws.onclose = () => {
        // Reaching here at all means this socket was NOT retired by us
        // (`retireSocket` detaches this handler first), so the close came
        // from the network or the server. That includes a clean 1000 from a
        // restarted backend or a proxy — previously skipped as if it were
        // our own close, which left the panel stuck OFFLINE with no retry.
        if (isUnmountedRef.current) return;
        setConnectionStatus('offline');

        // Exponential backoff reconnect
        const retryCount = retryCountRef.current;
        const delay = Math.min(1000 * Math.pow(1.5, retryCount), 10000);
        retryCountRef.current += 1;

        reconnectTimerRef.current = setTimeout(() => {
          if (!isUnmountedRef.current) {
            connectWs();
          }
        }, delay);
      };
    } catch {
      setConnectionStatus('offline');
    }
  }, [auth.status, projectId, retireSocket]);

  // Handle mounting and project changes
  useEffect(() => {
    isUnmountedRef.current = false;
    void fetchRecentLogs();
    connectWs();

    return () => {
      isUnmountedRef.current = true;
      abortControllerRef.current?.abort();
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
      retireSocket(wsRef.current);
      wsRef.current = null;
    };
  }, [fetchRecentLogs, connectWs, retireSocket]);

  // Filtered logs
  const filteredLogs = useMemo(() => {
    return logs.filter((log) => {
      if (levelFilter !== 'ALL' && log.level !== levelFilter) return false;
      if (searchQuery.trim()) {
        const query = searchQuery.toLowerCase();
        const matchMsg = log.message.toLowerCase().includes(query);
        const matchSource = log.source.toLowerCase().includes(query);
        const matchRun = log.run_id?.toLowerCase().includes(query) ?? false;
        if (!matchMsg && !matchSource && !matchRun) return false;
      }
      return true;
    });
  }, [logs, levelFilter, searchQuery]);

  const levelBadge = (level: string | undefined) => {
    // `isLogEntryShape` already guarantees a string, but this stays total on
    // its own so a future caller can never reintroduce the render-time crash
    // that took the whole Dashboard down.
    switch ((level ?? 'INFO').toUpperCase()) {
      case 'ERROR':
        return (
          <span className="px-1.5 py-0.5 rounded text-[9px] font-mono font-bold bg-rose-950/70 border border-rose-800/80 text-rose-300">
            ERROR
          </span>
        );
      case 'WARN':
        return (
          <span className="px-1.5 py-0.5 rounded text-[9px] font-mono font-bold bg-amber-950/70 border border-amber-800/80 text-amber-300">
            WARN
          </span>
        );
      case 'SUCCESS':
        return (
          <span className="px-1.5 py-0.5 rounded text-[9px] font-mono font-bold bg-emerald-950/70 border border-emerald-800/80 text-emerald-300">
            SUCCESS
          </span>
        );
      case 'DEBUG':
        return (
          <span className="px-1.5 py-0.5 rounded text-[9px] font-mono font-bold bg-slate-900 border border-slate-700 text-slate-400">
            DEBUG
          </span>
        );
      case 'INFO':
      default:
        return (
          <span className="px-1.5 py-0.5 rounded text-[9px] font-mono font-bold bg-yellow-950/70 border border-yellow-800/80 text-yellow-300">
            INFO
          </span>
        );
    }
  };

  return (
    <div
      id="aiqa-orchestrator-logs-panel"
      className={`rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-lg flex flex-col h-[640px] overflow-hidden ${className}`}
    >
      {/* 1. Header with Title and Connection Status */}
      <div className="px-3.5 py-2.5 border-b border-slate-800 bg-[#020617] flex items-center justify-between gap-2 shrink-0">
        <div className="flex items-center gap-2">
          <div className="p-1 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
            <Terminal className="w-3.5 h-3.5" />
          </div>
          <div>
            <h2 className="text-xs font-bold text-slate-100 font-mono uppercase tracking-wider">AIQA ORCHESTRATOR</h2>
            <div className="flex items-center gap-1.5 text-[9px] font-mono text-slate-400">
              <span>LIVE ACTIVITY</span>
              {projectId && (
                <>
                  <span>•</span>
                  <span className="text-violet-400 truncate max-w-[110px]" title={`Filtered for project ${projectId}`}>
                    Scoped
                  </span>
                </>
              )}
            </div>
          </div>
        </div>

        {/* Real Live Indicator Badge */}
        <div className="flex items-center gap-2">
          {connectionStatus === 'live' && (
            <div
              id="orchestrator-log-status-live"
              className="flex items-center gap-1.5 px-2 py-0.5 rounded bg-emerald-950/60 border border-emerald-800/60 text-[10px] font-mono font-bold text-emerald-400"
            >
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
              </span>
              <span>LIVE</span>
            </div>
          )}

          {(connectionStatus === 'connecting' || connectionStatus === 'reconnecting') && (
            <div
              id="orchestrator-log-status-connecting"
              className="flex items-center gap-1.5 px-2 py-0.5 rounded bg-amber-950/60 border border-amber-800/60 text-[10px] font-mono font-bold text-amber-400"
            >
              <Loader2 className="w-2.5 h-2.5 animate-spin" />
              <span>{connectionStatus === 'reconnecting' ? 'RECONNECTING' : 'CONNECTING'}</span>
            </div>
          )}

          {connectionStatus === 'offline' && (
            <button
              id="orchestrator-log-status-offline"
              onClick={connectWs}
              title="Click to reconnect stream"
              className="flex items-center gap-1.5 px-2 py-0.5 rounded bg-slate-900 hover:bg-slate-800 border border-slate-700 text-[10px] font-mono font-bold text-slate-400 hover:text-slate-200 cursor-pointer transition-colors"
            >
              <WifiOff className="w-2.5 h-2.5 text-slate-500" />
              <span>OFFLINE</span>
            </button>
          )}

          <button
            onClick={() => {
              void fetchRecentLogs();
              connectWs();
            }}
            title="Refresh logs & stream"
            className="p-1 text-slate-400 hover:text-slate-200 rounded hover:bg-slate-800 cursor-pointer transition-colors"
          >
            <RefreshCw className={`w-3 h-3 ${loadingInitial ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>

      {/* 2. Subheader Controls: Level Filter & Search */}
      <div className="px-3 py-1.5 border-b border-slate-800/80 bg-[#070D19] flex items-center justify-between gap-2 shrink-0 text-xs">
        <div className="flex items-center gap-1 overflow-x-auto no-scrollbar py-0.5">
          {(['ALL', 'INFO', 'WARN', 'ERROR', 'SUCCESS'] as LogLevelFilter[]).map((lvl) => (
            <button
              key={lvl}
              onClick={() => setLevelFilter(lvl)}
              className={`px-1.5 py-0.5 rounded text-[10px] font-mono font-semibold transition-colors cursor-pointer ${
                levelFilter === lvl
                  ? 'bg-violet-600 text-white shadow-sm'
                  : 'bg-slate-900/80 text-slate-400 hover:text-slate-200 hover:bg-slate-800'
              }`}
            >
              {lvl}
            </button>
          ))}
        </div>

        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder="Filter logs…"
          className="w-28 sm:w-36 px-2 py-0.5 rounded bg-[#020617] border border-slate-800 text-[11px] font-mono text-slate-200 placeholder:text-slate-600 focus:outline-none focus:border-violet-600"
        />
      </div>

      {/* 3. Log Stream Body */}
      <div
        id="orchestrator-logs-scroll-container"
        ref={containerRef}
        onScroll={handleScroll}
        className="flex-1 p-2.5 overflow-y-auto font-mono text-[11px] leading-relaxed space-y-1.5 bg-[#020617]/95 relative select-text"
      >
        {loadingInitial && logs.length === 0 && (
          <div className="h-full flex flex-col items-center justify-center text-slate-500 gap-2">
            <Loader2 className="w-5 h-5 animate-spin text-violet-400" />
            <span className="text-xs">Connecting to orchestrator stream…</span>
          </div>
        )}

        {!loadingInitial && errorMessage && logs.length === 0 && (
          <div className="h-full flex flex-col items-center justify-center text-center p-4 gap-2">
            <AlertCircle className="w-6 h-6 text-rose-400" />
            <span className="text-xs font-semibold text-rose-300">Orchestrator stream unavailable</span>
            <p className="text-[10px] text-slate-400 max-w-xs">{errorMessage}</p>
            <button
              onClick={() => {
                void fetchRecentLogs();
                connectWs();
              }}
              className="mt-1 px-2.5 py-1 rounded bg-violet-950 hover:bg-violet-900 border border-violet-700/60 text-violet-300 text-xs font-mono font-medium cursor-pointer"
            >
              Retry Connection
            </button>
          </div>
        )}

        {!loadingInitial && !errorMessage && logs.length === 0 && (
          <div className="h-full flex flex-col items-center justify-center text-center p-4 text-slate-500 gap-1.5">
            <Terminal className="w-6 h-6 text-slate-600" />
            <span className="text-xs font-semibold text-slate-300">No orchestrator activity yet</span>
            <p className="text-[10px] text-slate-500 max-w-xs leading-normal">
              Logs will appear here when AIQA processes generation, validation, routing, or execution activity.
            </p>
          </div>
        )}

        {filteredLogs.map((log) => (
          <div
            key={log.id}
            className="flex items-start gap-1.5 py-0.5 hover:bg-slate-900/60 px-1 rounded transition-colors group"
          >
            <span className="text-slate-500 text-[10px] shrink-0 select-none">
              {formatTimeOnly(log.timestamp)}
            </span>
            <div className="shrink-0">{levelBadge(log.level)}</div>
            <span className="text-slate-400 font-semibold text-[10px] shrink-0">
              [{log.source}]
            </span>
            <span className="text-slate-200 text-[11px] break-words flex-1 leading-snug">
              {log.message}
            </span>
            {log.run_id && (
              <span
                className="text-[9px] text-violet-400/80 bg-violet-950/40 px-1 py-0.2 rounded shrink-0 opacity-80 group-hover:opacity-100 truncate max-w-[70px]"
                title={`Run ID: ${log.run_id}`}
              >
                run:{log.run_id.slice(0, 6)}
              </span>
            )}
          </div>
        ))}
      </div>

      {/* 4. Bottom Footer with auto-scroll helper */}
      <div className="px-3 py-1.5 border-t border-slate-800/80 bg-[#020617] flex items-center justify-between text-[10px] font-mono text-slate-500 shrink-0">
        <div className="flex items-center gap-2">
          <span>{filteredLogs.length} events</span>
          {logs.length > filteredLogs.length && (
            <span className="text-slate-600">(filtered from {logs.length})</span>
          )}
        </div>

        {hasUnread && (
          <button
            onClick={scrollToBottom}
            className="flex items-center gap-1 px-2 py-0.5 rounded-full bg-violet-600 hover:bg-violet-500 text-white font-sans text-[10px] font-semibold cursor-pointer animate-bounce shadow-md"
          >
            <span>New logs available</span>
            <ArrowDown className="w-2.5 h-2.5" />
          </button>
        )}

        <div className="flex items-center gap-2 text-slate-500">
          <span>Auto-scroll</span>
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              isNearBottomRef.current ? 'bg-emerald-400' : 'bg-slate-600'
            }`}
          />
        </div>
      </div>
    </div>
  );
};

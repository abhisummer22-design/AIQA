/**
 * FE-9 — the real realtime/event architecture, replacing FE-8's always-on
 * 3-second polling with the backend's real project-scoped WebSocket stream
 * where it genuinely applies.
 *
 * Backend contract (re-verified by reading `services/orchestrator/src/
 * api.rs`, `domain.rs`, `engine.rs`, `ws_tests.rs` directly — not assumed
 * from a route name or from this sprint's own brief's illustrative
 * examples):
 *
 *   GET /api/v1/projects/:project_id/events?key=<raw key>   (project_events_ws_handler, BE-14)
 *
 * - **Real WebSocket** (axum `WebSocketUpgrade`), not SSE or any other
 *   streaming mechanism — the flow-diagram/architecture documents' own
 *   "WEBSOCKET" label (Flow Diagram v6.0 pages 5-6, Business Logic v2.0
 *   §11 Observability row) matches this exactly; no transport contradiction
 *   was found.
 * - **Auth is a `?key=` query parameter**, not the `Authorization` header
 *   every REST call uses — `ws_handler`'s own doc comment explains why:
 *   a browser's `new WebSocket(url)` takes no headers argument, so a
 *   custom header cannot be set on the handshake. This is not a second,
 *   weaker credential store: `projectEventsWsUrl()` (`services/aiqaApi.ts`)
 *   reads the exact same raw key `getApiKey()` already returns.
 * - **No backlog/replay** on this endpoint (unlike the legacy per-run
 *   `/runs/:id/ws`) — only events occurring *after* a successful
 *   subscription are delivered. Catch-up for a gap is the pre-existing
 *   per-run REST endpoint (`aiqaApi.executions.events`, `?since=<sequence>`)
 *   — there is no project-wide catch-up endpoint, so this provider does not
 *   invent one; see `handleReconnect` below and the FE-9 report §16/§17.
 * - **Real event envelope**: exactly `RunEvent`/`EventType`
 *   (`types/contract.ts`, unchanged — already an exact mirror of
 *   `domain.rs::RunEvent` since FE-1, confirmed field-for-field again this
 *   sprint). No new event type was invented.
 * - **Real event vocabulary actually emitted** (confirmed by reading every
 *   `record.emit(...)` call site in `engine.rs`): `RUN_SCHEDULED` (once,
 *   when a run actually starts executing — at creation *or* after
 *   approval, `record.state` is already `Running` by the time this fires),
 *   nine `TASK_*` events, and `RUN_COMPLETED` (once, terminal, for *every*
 *   reason a run ends — completed, partial, failed, cancelled, or
 *   rejected). There is **no dedicated approval/rejection/cancellation
 *   event type** — `reject_run` piggybacks its terminal signal onto the
 *   same `RUN_COMPLETED` type (distinguishable only by reading its
 *   `summary` string, e.g. `"run rejected by ..."`), and cancellation is
 *   visible only as per-task `TASK_CANCELLED` events followed by the same
 *   unified `RUN_COMPLETED`. Critically, `RUN_COMPLETED` never carries the
 *   run's actual final `RunState` (`status` is `null` on that event) — a
 *   consumer that needs to know COMPLETED vs PARTIAL vs FAILED vs
 *   CANCELLED vs REJECTED must re-fetch run detail. This is a genuine,
 *   disclosed real-backend limitation, not a gap in this provider.
 * - **Ordering/dedup**: `event_id` (UUID) is the real, globally-unique
 *   dedup key; `sequence` is a real per-run monotonic counter; the
 *   project-stream's own broadcast copy additionally carries
 *   `project_sequence`, a real project-wide monotonic counter (`null` on
 *   the per-run REST catch-up copy — see that field's own doc comment in
 *   `types/contract.ts`). This provider deduplicates by `event_id`;
 *   ordering/staleness guards against a *specific* piece of UI state are a
 *   consumer concern (e.g. `ExecutionHubPage` comparing `sequence` against
 *   the highest one already applied to its selected run) — this provider
 *   only guarantees delivery-order-preserving, deduplicated forwarding.
 * - **Lag handling**: a subscriber that falls behind the server's
 *   1024-capacity broadcast channel is closed with a real, distinct close
 *   code `4001` ("lagged behind... reconnect and catch up via REST") —
 *   confirmed by `ws_tests.rs::be14_lagging_subscriber_is_closed_with_
 *   distinct_code_not_silently_skipped`. This provider treats `4001`
 *   specially: an uncounted, immediate reconnect (it is a real signal, not
 *   a failure).
 * - **A real, disclosed browser-imposed limitation (backend's own "G-WS")**:
 *   pre-upgrade handshake rejections (401 missing/invalid key, 403 wrong
 *   project scope, 404 unknown project) are real, plain-text HTTP responses
 *   server-side (confirmed directly against a real TCP socket in
 *   `ws_tests.rs`) — but the browser's native `WebSocket` API does not
 *   expose a failed handshake's HTTP status code to JavaScript at all (only
 *   a generic `error` event, then `close`, per the WHATWG spec — this is a
 *   platform limitation, not something this backend or this provider can
 *   fix). This provider therefore cannot distinguish "wrong key" from
 *   "wrong project" from "server down" from a failed WS connection alone —
 *   see `lastError`'s own doc comment and the FE-9 report §18.
 *
 * Architecture: exactly **one** connection, centralized here, driven
 * entirely by `AuthContext.status` + `ProjectContext.selectedProjectId` —
 * no page/component opens its own socket. Pages subscribe via
 * `useRealtime().subscribe(listener)`.
 */

import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { projectEventsWsUrl } from '../services/aiqaApi';
import type { RunEvent } from '../types/contract';
import { useAuth } from './AuthContext';
import { useProject } from './ProjectContext';

export type RealtimeConnectionState = 'disconnected' | 'connecting' | 'connected' | 'reconnecting' | 'error';

export type RealtimeEventListener = (event: RunEvent) => void;

export interface RealtimeContextValue {
  connectionState: RealtimeConnectionState;
  /** Real ISO timestamp (`RunEvent.timestamp`) of the last event actually
   *  received and accepted (post-validation, post-isolation, post-dedup).
   *  `null` until the first one arrives this session. */
  lastEventAt: string | null;
  /** The most recent connection-level failure, if any. Deliberately a
   *  plain `Error`, never a fabricated `ApiError` — a WS handshake failure
   *  carries no real HTTP status this provider can classify (see the
   *  module doc comment's G-WS note); inventing an `ApiErrorKind` for it
   *  would misrepresent information this provider does not actually have. */
  lastError: Error | null;
  /** Manual retry (e.g. an error-state `Cta`). Resets the backoff counter
   *  and attempts a fresh connection immediately. No-op outside
   *  `authenticated` + a selected project — the provider otherwise manages
   *  connect/disconnect automatically. */
  reconnect: () => void;
  /** Subscribe to every real, validated, project-isolated, deduplicated
   *  event on the current project's stream. Returns an unsubscribe
   *  function. A throwing listener is caught and logged — it never takes
   *  down the socket or other subscribers. */
  subscribe: (listener: RealtimeEventListener) => () => void;
}

const RealtimeContext = createContext<RealtimeContextValue | undefined>(undefined);

const MAX_RECONNECT_ATTEMPTS = 6;
const BASE_RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_DELAY_MS = 30000;
/** Bounded so long-lived sessions with many runs don't grow this forever —
 *  large enough that a real reconnect/lag burst never wraps around and
 *  re-admits a genuine duplicate. */
const DEDUP_CACHE_SIZE = 500;

function isRunEventShape(value: unknown): value is RunEvent {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.event_id === 'string' &&
    typeof v.run_id === 'string' &&
    typeof v.sequence === 'number' &&
    typeof v.timestamp === 'string' &&
    typeof v.type === 'string' &&
    (v.task_id === null || v.task_id === undefined || typeof v.task_id === 'string') &&
    (v.project_id === null || v.project_id === undefined || typeof v.project_id === 'string')
  );
}

export const RealtimeProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const auth = useAuth();
  const project = useProject();

  const [connectionState, setConnectionState] = useState<RealtimeConnectionState>('disconnected');
  const [lastEventAt, setLastEventAt] = useState<string | null>(null);
  const [lastError, setLastError] = useState<Error | null>(null);

  const socketRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const intentionalCloseRef = useRef(false);
  const currentProjectIdRef = useRef<string | null>(null);
  const selectedProjectIdRef = useRef<string | null>(null);
  const listenersRef = useRef<Set<RealtimeEventListener>>(new Set());
  const seenEventIdsRef = useRef<Set<string>>(new Set());
  const eventOrderRef = useRef<string[]>([]);

  useEffect(() => {
    selectedProjectIdRef.current = project.selectedProjectId;
  }, [project.selectedProjectId]);

  function teardownSocket() {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }
    if (socketRef.current) {
      intentionalCloseRef.current = true;
      try {
        socketRef.current.close();
      } catch {
        /* already closed/closing */
      }
      socketRef.current = null;
    }
  }

  function handleIncoming(raw: unknown, socketProjectId: string) {
    if (typeof raw !== 'string') return; // binary frames are not part of the real contract — ignored
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      return; // malformed JSON — safely ignored, never crashes the app (brief §10/§32)
    }
    if (!isRunEventShape(parsed)) return; // unknown/malformed shape — safely ignored
    const event = parsed;

    // Defense-in-depth project isolation (brief §11) — the backend already
    // enforces this server-side (confirmed by
    // `ws_tests.rs::be14_subscribers_receive_only_their_own_projects_
    // real_events`); this is a *second*, client-side check against
    // whatever project is *currently* selected, read fresh from a ref so a
    // fast project switch can never let a just-superseded socket's event
    // reach a listener for the new project.
    if (selectedProjectIdRef.current !== socketProjectId) return;
    if (event.project_id !== null && event.project_id !== selectedProjectIdRef.current) return;

    if (seenEventIdsRef.current.has(event.event_id)) return; // real dedup key (brief §27)
    seenEventIdsRef.current.add(event.event_id);
    eventOrderRef.current.push(event.event_id);
    if (eventOrderRef.current.length > DEDUP_CACHE_SIZE) {
      const oldest = eventOrderRef.current.shift();
      if (oldest) seenEventIdsRef.current.delete(oldest);
    }

    setLastEventAt(event.timestamp);
    for (const listener of listenersRef.current) {
      try {
        listener(event);
      } catch (err) {
        // A subscriber's own bug must never take down the socket or other
        // subscribers (brief §10/§32).
        // eslint-disable-next-line no-console
        console.error('RealtimeContext: a subscriber threw while handling an event', err);
      }
    }
  }

  function scheduleReconnect(projectId: string) {
    const attempt = reconnectAttemptsRef.current;
    if (attempt >= MAX_RECONNECT_ATTEMPTS) {
      setConnectionState('error');
      return;
    }
    setConnectionState('reconnecting');
    const delay = Math.min(BASE_RECONNECT_DELAY_MS * 2 ** attempt, MAX_RECONNECT_DELAY_MS);
    reconnectAttemptsRef.current = attempt + 1;
    reconnectTimeoutRef.current = setTimeout(() => {
      reconnectTimeoutRef.current = null;
      openSocket(projectId, true);
    }, delay);
  }

  function openSocket(projectId: string, isReconnectAttempt: boolean) {
    teardownSocket(); // never more than one socket at a time (brief §8, §24)
    intentionalCloseRef.current = false;
    currentProjectIdRef.current = projectId;

    const url = projectEventsWsUrl(projectId);
    if (!url) {
      setConnectionState('disconnected');
      return;
    }

    setConnectionState(isReconnectAttempt ? 'reconnecting' : 'connecting');

    let socket: WebSocket;
    try {
      socket = new WebSocket(url);
    } catch (err) {
      setLastError(err instanceof Error ? err : new Error(String(err)));
      scheduleReconnect(projectId);
      return;
    }
    socketRef.current = socket;

    socket.onopen = () => {
      if (socketRef.current !== socket) return; // superseded before it finished opening
      reconnectAttemptsRef.current = 0;
      setConnectionState('connected');
      setLastError(null);
    };

    socket.onmessage = (ev) => {
      if (socketRef.current !== socket) return;
      handleIncoming(ev.data, projectId);
    };

    socket.onerror = () => {
      // Real browsers give no usable detail here — see the module doc
      // comment's G-WS note. `onclose` (which always follows) drives the
      // actual state transition, so this is intentionally a no-op beyond
      // what `onclose` already does.
    };

    socket.onclose = (ev) => {
      if (socketRef.current !== socket) return; // an already-superseded socket closing late
      socketRef.current = null;
      if (intentionalCloseRef.current) {
        intentionalCloseRef.current = false;
        return; // we closed this ourselves (project switch / sign-out / unmount) — no reconnect
      }
      if (ev.code === 4001) {
        // BE-14 lag signal — a real, explicit "you may have missed events"
        // notice, not a failure. Reconnect immediately, uncounted against
        // the failure backoff.
        reconnectAttemptsRef.current = 0;
        openSocket(projectId, true);
        return;
      }
      setLastError(new Error(ev.reason || `Realtime connection closed (code ${ev.code}).`));
      scheduleReconnect(projectId);
    };
  }

  // The one real connect/disconnect lifecycle — driven only by auth status
  // and the selected project (brief §6, §29). Every other status
  // (`no_key`, `key_present`, `checking`, `unauthenticated`, `forbidden`,
  // `offline`, `error`) disconnects: this provider does not attempt to
  // distinguish "backend genuinely unreachable" from "one unrelated
  // validation call happened to bounce" the way `ProjectContext` does for
  // its own (inert, easily-restored) project list — a live socket is a
  // stateful resource, and tearing it down is cheap and self-healing once
  // `authenticated` is reproven. A disclosed simplification, not an
  // oversight (FE-9 report §7).
  useEffect(() => {
    const shouldConnect = auth.status === 'authenticated' && !!project.selectedProjectId;
    if (!shouldConnect) {
      teardownSocket();
      reconnectAttemptsRef.current = 0;
      currentProjectIdRef.current = null;
      setConnectionState('disconnected');
      setLastError(null);
      return;
    }
    const projectId = project.selectedProjectId as string;
    if (currentProjectIdRef.current === projectId && (socketRef.current || reconnectTimeoutRef.current)) {
      return; // already connected/connecting/reconnecting to this exact project — never a duplicate
    }
    reconnectAttemptsRef.current = 0;
    openSocket(projectId, false);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- openSocket/teardownSocket close over refs only, safe to omit
  }, [auth.status, project.selectedProjectId]);

  // Unmount cleanup (brief §28) — belt-and-suspenders on top of the effect
  // above's own cleanup path (which already covers sign-out/auth-loss/
  // project-switch via the dependency change itself).
  useEffect(() => () => teardownSocket(), []);

  const reconnect = useCallback(() => {
    if (auth.status !== 'authenticated' || !project.selectedProjectId) return;
    reconnectAttemptsRef.current = 0;
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }
    openSocket(project.selectedProjectId, false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth.status, project.selectedProjectId]);

  const subscribe = useCallback((listener: RealtimeEventListener) => {
    listenersRef.current.add(listener);
    return () => {
      listenersRef.current.delete(listener);
    };
  }, []);

  const value = useMemo<RealtimeContextValue>(
    () => ({ connectionState, lastEventAt, lastError, reconnect, subscribe }),
    [connectionState, lastEventAt, lastError, reconnect, subscribe],
  );

  return <RealtimeContext.Provider value={value}>{children}</RealtimeContext.Provider>;
};

export const useRealtime = (): RealtimeContextValue => {
  const ctx = useContext(RealtimeContext);
  if (!ctx) {
    throw new Error('useRealtime must be used within a RealtimeProvider');
  }
  return ctx;
};

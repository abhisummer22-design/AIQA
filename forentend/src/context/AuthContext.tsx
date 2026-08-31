/**
 * FE-2 — real frontend authentication state, built on the FE-1 API key
 * storage abstraction (`services/aiqaApi.ts`: `getApiKey`/`setApiKey`/
 * `clearApiKey`/`hasApiKey`/`maskApiKey`) and its `ApiError` classification.
 *
 * This is the *one* source of truth for "is this browser authenticated" —
 * every component that needs to know reads it from here, none of them
 * touch `localStorage` directly (Integration Plan §9.3 item 1 / this
 * sprint's brief §5, §7).
 *
 * Backend model (Integration Plan §9.1, unchanged, not reimplemented here):
 * opaque `aiqa_<hex>` bearer keys, hashed server-side, three fixed roles
 * (viewer/operator/admin), optional project scope. No user/password model,
 * no OAuth, no sessions — this frontend does not invent one either.
 *
 * Deliberately does NOT distinguish RBAC permissions on the frontend
 * (Contract §26.8 / this sprint's brief §7: "frontend checks are UX only;
 * backend remains authoritative"). It only classifies what the backend's
 * own HTTP response said.
 */

import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { ApiError, aiqaApi, clearApiKey, getApiKey, hasApiKey, maskApiKey, setApiKey } from '../services/aiqaApi';

export type AuthStatus =
  /** No key has ever been saved. */
  | 'no_key'
  /** A key is stored locally but has not (yet, or not successfully) been
   *  confirmed against the backend this session. Distinct from
   *  `authenticated` — storage presence is not proof of validity (this
   *  sprint's brief §9: "Do not trust localStorage alone"). */
  | 'key_present'
  /** A validation request is in flight. */
  | 'checking'
  /** The backend accepted a real protected request with this key. */
  | 'authenticated'
  /** The backend returned 401 for this key: unknown, revoked, or malformed. */
  | 'unauthenticated'
  /** The backend returned 403: a real, recognized identity that this
   *  action/resource does not permit. NOT the same problem as 401 — the
   *  key stays configured (this sprint's brief §8, §12). */
  | 'forbidden'
  /** The orchestrator could not be reached at all (network/DNS/proxy-down).
   *  Never collapsed into 401/403 (this sprint's brief §12). */
  | 'offline'
  /** Any other failure (5xx, unexpected shape). */
  | 'error';

export interface AuthContextType {
  status: AuthStatus;
  /** Convenience: `status !== 'no_key'`. */
  hasKey: boolean;
  /** `aiqa_••••••••••••1234`-shaped, safe to render. `null` when no key. */
  maskedKey: string | null;
  /** The most recent `ApiError` from a validation attempt, if any. Cleared
   *  on `saveKey`/`signOut`. */
  lastError: ApiError | null;
  /** Stores a new key (replacing any existing one — this sprint's brief
   *  §11: only one active credential at a time) and immediately validates
   *  it against the backend. */
  saveKey: (rawKey: string) => Promise<void>;
  /** Re-runs validation against the currently stored key. No-op (stays
   *  `no_key`) if none is stored. */
  validate: () => Promise<void>;
  /** Clears the stored key and all auth state. Touches nothing else —
   *  no project/run/dashboard state (this sprint's brief §10). */
  signOut: () => void;
}

const AuthContext = createContext<AuthContextType | undefined>(undefined);

/**
 * §9 validation endpoint: `GET /api/v1/projects` (`aiqaApi.projects.list`).
 *
 * Chosen because it is the lightest real, already-implemented, protected
 * endpoint that places no precondition on the caller (no project id needs
 * to be known up front, unlike every `/api/v1/projects/{id}/...` route),
 * and it accepts every role (viewer/operator/admin) — so "the key is
 * valid" and "the key is authorized to check its own validity" are the
 * same question here, which is exactly what this check needs to answer.
 *
 * Known, documented limitation: because this route never itself denies an
 * authenticated caller (BE-8's `list_projects_handler` only filters by the
 * caller's own scope, it does not reject), this validation call can only
 * ever observe `authenticated`/`unauthenticated`/`offline`/`error` — never
 * `forbidden`. A real 403 does happen on this backend (any role-gated or
 * cross-project request — e.g. a viewer-scoped key calling the
 * project-scoped audit endpoint, which requires `operator`), and is
 * classified correctly by the same `ApiError`/`kindForStatus` machinery
 * this context uses — see the FE-2 report §7 for the exact backend proof
 * and why wiring a live one-click 403 demo here would mean wiring a real
 * role/project-gated action, which is explicitly out of this sprint's
 * scope (that is FE-6 onward's job).
 */
async function checkAuth(): Promise<void> {
  await aiqaApi.projects.list();
}

export const AuthProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [status, setStatus] = useState<AuthStatus>(() => (hasApiKey() ? 'key_present' : 'no_key'));
  const [lastError, setLastError] = useState<ApiError | null>(null);

  const validate = useCallback(async () => {
    if (!hasApiKey()) {
      setStatus('no_key');
      setLastError(null);
      return;
    }
    setStatus('checking');
    try {
      await checkAuth();
      setStatus('authenticated');
      setLastError(null);
    } catch (err) {
      // A non-`ApiError` throw here would mean a genuine unexpected bug
      // (aiqaApi's transport core always throws `ApiError`), not a
      // classified backend response — `server_error` is the closest honest
      // fallback kind, not a fabricated new one.
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setLastError(apiErr);
      switch (apiErr.kind) {
        case 'unauthorized':
          setStatus('unauthenticated');
          break;
        case 'forbidden':
          setStatus('forbidden');
          break;
        case 'offline':
          setStatus('offline');
          break;
        default:
          setStatus('error');
      }
    }
  }, []);

  const saveKey = useCallback(
    async (rawKey: string) => {
      setApiKey(rawKey);
      setLastError(null);
      // A key was just supplied (or replaced — this sprint's brief §11:
      // the old credential is gone the instant this call returns, there is
      // never more than one stored at a time), so the previous auth
      // verdict no longer applies to it.
      setStatus('key_present');
      await validate();
    },
    [validate],
  );

  const signOut = useCallback(() => {
    clearApiKey();
    setStatus('no_key');
    setLastError(null);
  }, []);

  const maskedKey = useMemo(() => {
    const key = getApiKey();
    return key ? maskApiKey(key) : null;
  }, [status]); // eslint-disable-line react-hooks/exhaustive-deps -- storage is the real source; `status` changing is our signal to re-read it, not a value we derive from.

  const value = useMemo<AuthContextType>(
    () => ({
      status,
      hasKey: status !== 'no_key',
      maskedKey,
      lastError,
      saveKey,
      validate,
      signOut,
    }),
    [status, maskedKey, lastError, saveKey, validate, signOut],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};

export const useAuth = (): AuthContextType => {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return ctx;
};

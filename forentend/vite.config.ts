/// <reference types="vitest/config" />
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import path from 'path';
import {defineConfig} from 'vite';

/**
 * FE-1 — dev-server proxy to the real AIQA orchestrator
 * (`services/orchestrator`, Rust/Axum, default http://127.0.0.1:8080).
 *
 * Unlike `apps/web`'s equivalent proxy (which strips the `/api` prefix,
 * because that reference app's client only ever calls the orchestrator's
 * legacy flat routes like `/runs`), this proxy forwards `/api/v1/*`
 * unchanged - `services/aiqaApi.ts` calls the real, contract-shaped
 * `/api/v1/projects/{project_id}/...` routes BE-8..BE-15 mount at that
 * exact path, so nothing needs rewriting. `ws: true` covers BE-14's
 * project-scoped realtime stream for when FE-9 wires it up.
 *
 * Point `VITE_ORCHESTRATOR_URL` at a different host to develop against a
 * remote orchestrator. In production, put a reverse proxy in front of the
 * built assets that forwards `/api/v1` to the orchestrator the same way -
 * `services/aiqaApi.ts`'s own default base path (`/api/v1`) is already
 * same-origin-relative for exactly this.
 */
const ORCHESTRATOR_URL = process.env.VITE_ORCHESTRATOR_URL ?? 'http://127.0.0.1:8080';

export default defineConfig(() => {
  return {
    plugins: [react(), tailwindcss()],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, '.'),
      },
    },
    server: {
      // HMR is disabled in AI Studio via DISABLE_HMR env var.
      // Do not modify - file watching is disabled to prevent flickering during agent edits.
      hmr: process.env.DISABLE_HMR !== 'true',
      // Disable file watching when DISABLE_HMR is true to save CPU during agent edits.
      watch: process.env.DISABLE_HMR === 'true' ? null : {},
      proxy: {
        '/api/v1': {
          target: ORCHESTRATOR_URL,
          changeOrigin: true,
          ws: true,
          configure: (proxy) => {
            // Without this, an unreachable orchestrator surfaces to the
            // browser as a generic 500, which `aiqaApi.ts` would report as
            // "the orchestrator returned an internal error" - a materially
            // different (and wrong) statement from "the orchestrator is not
            // running". 502 is the accurate code for "this proxy could not
            // reach upstream", and `kindForStatus` maps it to `offline`.
            //
            // FE-9 live-discovered fix: this `error` event also fires for
            // the *WebSocket* upgrade proxy (BE-14's realtime stream) when
            // the orchestrator dies mid-connection - but there, `res` is a
            // raw `net.Socket`, not an `http.ServerResponse` (no
            // `writeHead`/`end`), so the `typeof` guard below always
            // skipped it. That silently left the browser-facing socket
            // technically still open: `RealtimeContext`'s `WebSocket` never
            // received a real `close` event, so its connection-state
            // indicator kept showing "Live" indefinitely after a real
            // backend crash - confirmed live during FE-9 manual
            // verification (Scenario I), where the UI stayed on "Live" for
            // over a minute after the real orchestrator process was
            // killed, with the dev server's own console already logging
            // repeated `ws proxy socket error: ECONNRESET` for the dead
            // upstream connection the whole time. Destroying the socket
            // here makes the proxy's own already-detected failure visible
            // to the browser, so the real `WebSocket.onclose` (and this
            // provider's real bounded-reconnect path) actually fires.
            proxy.on('error', (_err, _req, res) => {
              const response = res as unknown as {
                writeHead?: (code: number, headers: Record<string, string>) => void;
                end?: (body: string) => void;
                destroy?: (error?: Error) => void;
              };
              if (typeof response.writeHead === 'function' && typeof response.end === 'function') {
                response.writeHead(502, {'Content-Type': 'text/plain'});
                response.end('orchestrator unreachable');
              } else if (typeof response.destroy === 'function') {
                response.destroy();
              }
            });
          },
        },
      },
    },
    // FE-2: no test framework existed in this project before this sprint
    // (Integration Plan T-3 / this brief §18: "the lightest appropriate
    // test mechanism"). Vitest is Vite's own test runner — zero extra
    // config beyond this block, reuses the exact same transform pipeline
    // (TS, the `@` alias, JSX) the app itself builds with.
    test: {
      environment: 'jsdom',
      globals: true,
    },
  };
});

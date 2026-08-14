import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import path from 'path';
import { defineConfig } from 'vite';

/**
 * The UI is served by Vite and talks to exactly one backend: the Rust
 * orchestrator in `services/orchestrator` (default http://127.0.0.1:8080).
 *
 * There is deliberately no Node/Express server in this app any more. The
 * previous `server.ts` implemented 59 routes that fabricated every value
 * they returned; it has been removed so the UI cannot show data that no
 * real backend produced.
 *
 * `/api` is proxied (with the prefix stripped) so browser code only ever
 * issues same-origin requests and no CORS configuration is required in dev.
 * `ws: true` covers the orchestrator's `/runs/:id/ws` event socket.
 *
 * Point `VITE_ORCHESTRATOR_URL` at a different host to develop against a
 * remote orchestrator. In production, put a reverse proxy in front of the
 * built assets that forwards `/api` to the orchestrator the same way.
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
      hmr: process.env.DISABLE_HMR !== 'true',
      watch: process.env.DISABLE_HMR === 'true' ? null : {},
      proxy: {
        '/api': {
          target: ORCHESTRATOR_URL,
          changeOrigin: true,
          ws: true,
          rewrite: (p) => p.replace(/^\/api/, ''),
        },
      },
    },
  };
});

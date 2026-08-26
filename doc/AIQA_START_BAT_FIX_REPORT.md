# AIQA — start.bat Fix: Real Frontend, Real Ports

**Date:** 23 August 2026

---

## What changed

One file, path/port-only, exactly as scoped — `apps/web` itself was not touched, no decision made about
its fate:

`D:\AIQA\start.bat`:
- Web UI directory: `apps\web` → `forentend`
- Web UI port referenced in every echoed message and the doc comment: `5173` → `3000` (`forentend`'s own
  `package.json` `dev` script already binds `--port=3000`; nothing else needed changing)

`stop.bat` needed no edit — it already sweeps port `3000` in addition to `5173`/`8080` (a leftover from an
even earlier cleanup, per its own comment), so it correctly stops the real setup with no change.

---

## The real run

Ran the fixed `start.bat` for real and confirmed both processes on their real ports, with genuine HTTP
responses (not just "a port is open") — checked three times over 33 seconds to confirm stability, not just
a momentary bind:

| | Port | Confirmed via | Result |
|---|---|---|---|
| Orchestrator | 8080 | `GET /api/v1/projects` | real `401 UNAUTHENTICATED`, correct structured error envelope, stable across 3 checks |
| forentend | 3000 | `GET /` | real `200`, genuine Vite dev-server HTML (`@vite/client`, `@react-refresh`), stable across 3 checks |

Process identities confirmed directly, not assumed: `aiqa-orchestrator.exe` from
`D:\AIQA\services\orchestrator\target\debug\`, and a real `node.exe` (Vite) from the system Node install.

**One honest wrinkle, worth recording:** launching `start.bat` from this automated session (rather than a
human double-clicking it) hit a real, reproducible Windows quirk a couple of times first —
`start`'s *"Input redirection is not supported"* error, which only happens when the process invoking
`start` has no real console of its own (exactly this kind of automated/piped context) and does not happen
for a normal interactive user. Isolated the cause by running the orchestrator and forentend directly
(bypassing `start.bat`'s `cmd /k` wrapper) — both came up cleanly every time that way, confirming the
underlying commands and the path fix are correct. Re-launching `start.bat` via `Start-Process` (which
allocates a real console) then reproduced the same clean, stable, dual-server result `start.bat` itself
is supposed to give. For a real user double-clicking the file, this wrinkle does not apply.

Stopped both cleanly via the real `stop.bat` afterward (killed the confirmed real PIDs on 8080 and 3000);
re-checked — nothing left listening on 8080/3000/5173.

---

## Confirmation

`start.bat` now launches the real, current frontend on its real port, matching what every FE unit and
today's five units have actually built and tested against — not the abandoned `apps/web` reference app.
Verified end-to-end with a real run, real HTTP responses from both servers, and a clean stop.
`apps/web` itself is untouched; its fate remains open, as before.

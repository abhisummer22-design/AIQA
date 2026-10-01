# AIQA — Session Handoff (1 October 2026)

A short, accurate description of **this** checkout. Read it before changing
anything; everything here was verified against the working tree on the date
above. Run `git status` for the live picture — the working tree currently has
uncommitted modifications, and this file deliberately does not enumerate them
(they go stale).

## 1. Where things are

There is **one** checkout and **one** worktree:

- **Repo root**: `H:\AIQA`
- **Branch**: `main`
- **Worktrees**: just the root (`git worktree list` shows only `H:/AIQA`).

There is no second copy of the repo on disk, no symlinked mirror, and no
separate preview directory to keep in sync. Edit a file here and the dev
servers pick it up.

## 2. How to run it

| Piece | What it is | Port |
| :--- | :--- | :--- |
| `services/orchestrator` | The backend: Rust, Axum, SQLite | `8080` |
| `forentend` | The control plane: React 19 + Vite (`npm run dev` = `vite --port=3000 --host=0.0.0.0`) | `3000` |

```cmd
start.bat
```

Starts both. It clears stale processes on ports `8080`, `3000` and `5173`,
loads the root `.env`, installs `forentend/node_modules` if missing, starts the
orchestrator, polls `GET /health` until it answers, then starts Vite and opens
`http://localhost:3000`.

```cmd
stop.bat
```

Terminates everything listening on ports `8080`, `3000` and `5173`.

## 3. Persistence

The backend's SQLite store is `infra/data/orchestrator.sqlite3` (plus its
`-wal` / `-shm` siblings). It is **gitignored** (`infra/data/*.sqlite3`) — it is
runtime data, not source, and is never committed. Captured evidence blobs under
`infra/data/evidence/` are gitignored for the same reason.

Schema: sqlx applies **only** `services/orchestrator/migrations/` (0001–0026,
SQLite). `infra/migrations/` (001–020) is the Postgres target schema and is
**not applied** by the running service — do not treat the two as copies of each
other.

## 4. Configuration

AIQA's local configuration lives in a **gitignored root `.env`** (`/.env` in
`.gitignore`). `start.bat` loads it; nothing is committed. The variables set in
it today, by name only:

| Variable | Purpose |
| :--- | :--- |
| `GROQ_API_KEY` | Groq Cloud credential. **Never print, copy or commit this value.** |
| `GROQ_BASE_URL` | Groq's OpenAI-compatible base URL |
| `GROQ_MAX_TOKENS` | Largest single generation request Groq will accept |
| `GROQ_TIMEOUT_SECS` | Groq request timeout |
| `AIQA_MAX_CONCURRENCY` | Scheduler concurrency budget |
| `AIQA_TASK_TIMEOUT_SECS` | Hard per-task execution ceiling |
| `AIQA_REQUIRE_APPROVAL_FOR` | Comma-separated gated executors (e.g. `maestro`) |
| `AIQA_BLOCKED_EXECUTORS` | Executors to refuse outright |
| `AIQA_CORS_ALLOWED_ORIGINS` | Browser origins allowed to call the API |

Refer to these by name. No key value belongs in this file, in a commit, or in a
log. `README.md` §16 is the full reference for every supported variable,
including the ones not set here.

## 5. AI routing as it is configured now

- Mode: **`AUTO`** — the router ranks live providers by capability, token limit
  and health latency, and cascades to the next one on timeout or a 429.
- `preferred_model`: **`openai/gpt-oss-120b`**, served by **Groq**.
- So **Groq answers test-case generation**, with **`qwen3:14b`** on **Ollama** as
  the fallback.
- Ollama is reached at **`http://localhost:11434`** — the built-in default in
  `ai_router.rs` (no `OLLAMA_BASE_URL` is set). It is the local machine, not a
  remote host.

Settings live in the `ai_settings` table, so they survive restarts; the Settings
page and `ai_usage.rs` report what the router actually did.

## 6. Test counts

| Suite | Where | Count |
| :--- | :--- | :--- |
| Backend | `services/orchestrator` | **1,611** passing (2 `#[ignore]`d; 1,613 total) |
| Frontend | `forentend` | **~1,485** tests across **70** test files |

```cmd
cd services\orchestrator && cargo test -p aiqa-orchestrator
cd forentend && npm test
```

## 7. Honest limitations still in the code

- **Scanned / image-only PDFs are not read.** Text-based PDFs are extracted for
  real (`pdf-extract`); a PDF with no text layer is refused with a specific
  message saying it may be a scan.
- **Legacy `.doc`** (pre-2007 binary Word) is unsupported and refused with a
  "save it as .docx" message.
- Extracted document text is capped at **20,000 characters per document**;
  truncation is disclosed in the API response and in the UI.
- **The audit log is append-only by convention only.** `audit_log` has no hash
  chain and no signature, so it is **not tamper-evident**.
- **`feature-map`** is the one page with no backend at all; every other route
  has a real endpoint behind it (see `forentend/src/routes/routeConfig.tsx`).

## 8. Working rules

- Do not commit or push without being asked.
- Do not put a real API key into any file, commit, or log — reference the
  variable name.
- When a doc claim and the source disagree, the source wins; fix the doc.

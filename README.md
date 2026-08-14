# AIQA Agent Platform

AI-driven quality engineering control plane. Implements the architecture described in
`AIQA_Agent_PRD_FRD_SRD_v3.0.pdf` — section references throughout this codebase (`PRD §n` /
`FRD §n` / `SRD §n`) point back at that document.

## Status

This is a vertical slice, not the full platform, but a substantially deeper one than the initial
build: persistence, auth/governance, evidence, and a heuristic intelligence layer are now real and
live-verified, not just scaffolding.

- Orchestrator run/task state machine, dependency-aware scheduler, continue-on-failure execution,
  real-time event stream, live dashboard (FRD §1-9, SRD §2-6, acceptance test SRD §25)
- Capability routing (PRD §21) to real executors, each shelling out to the actual tool:
  **Playwright** (web), **Newman** (Postman/API), **HTTPX/Pytest** (native API), **Schemathesis**
  (property-based API fuzzing) — all four verified end-to-end against the orchestrator's own
  running server. **Maestro** (mobile) is a real adapter too, but unverifiable in this dev
  environment — see `executors/maestro/README.md`. Tasks that don't name an `executor` still run
  against the built-in `SimulatedExecutor`, which is what the SRD §25 demo uses.
- **Durable persistence** (`store.rs`): every run/task/event/evidence/API-key/audit-log row
  survives a process restart, backed by SQLite (not Postgres — see "Persistence" below).
- **Auth, roles, multi-tenancy** (`auth.rs`): bearer API keys, three roles
  (admin/operator/viewer), project scoping. Every route except `/health` requires a key.
- **AI governance / policy engine** (`policy.rs`): configurable approval gates
  (`AIQA_REQUIRE_APPROVAL_FOR`, default `maestro`) hold a run in `PENDING_APPROVAL` until an
  operator calls `POST /runs/:id/approve` or `/reject`.
- **Evidence capture** (`evidence.rs`): full stdout/stderr from every real executor invocation,
  stored on local disk (not MinIO — see "Persistence" below) and served via
  `GET /evidence/:id/content`.
- **Heuristic quality-graph / risk / flakiness / failure-cluster / bug-drafting intelligence**
  (`intelligence.rs`): rule-based (not LLM-backed — see below), runs after every completed run,
  computes risk/value/flakiness scores and clusters recurring failures into bug drafts that stay
  `NEEDS_REVIEW` until a human calls `POST /bug-drafts/:id/review`.
- Audit log, `GET /metrics` (Prometheus format), and a completion webhook (`AIQA_WEBHOOK_URL`).

**Not yet implemented** (deliberately, pending the open product decisions raised in the PRD
critique — AI model strategy, distributed-worker architecture): Ollama/LLM integration (so
root-cause hypotheses and bug-draft prose are rule-based, not model-generated), Redis-based
distributed workers (`services/workers/` is still a contract stub — this orchestrator runs
single-process/single-machine), Appium/Strix/k6 (post-MVP per PRD §22), and container isolation
for executors (they run as plain host subprocesses, not sandboxed per SRD §22).

### Persistence: SQLite/local-disk, not Postgres/MinIO

`infra/docker-compose.yml` (Postgres+pgvector, Redis, MinIO) and `infra/migrations/` describe the
target architecture, but there's no container runtime in this dev environment (`docker`/`podman`
absent from PATH) to actually run them. `services/orchestrator/migrations/` has a SQLite mirror of
that schema that the running service actually uses (`store.rs`), and `evidence.rs` stores blobs on
local disk under `infra/data/evidence/` instead of MinIO. Both are real, ACID/durable, and column-
for-column equivalent to the Postgres target — swapping the backend later is a driver change, not
a redesign.

## Layout

```
apps/web/               Next.js live dashboard (FRD §6-7)
services/orchestrator/   Rust/Axum control plane: state machine, scheduler, executor registry, API (SRD §1-6, §16-17)
services/workers/        Python worker-fleet contract notes for the FUTURE distributed architecture (SRD §1) — not what's wired in today, see below
executors/                Real per-tool projects the orchestrator shells out to: playwright/, newman/, pytest/, maestro/
infra/                   docker-compose.yml (Postgres+pgvector, Redis, MinIO) + SQL migrations (SRD §19)
```

`services/workers/` and `executors/` represent two different ideas, worth not conflating: the SRD
envisions a distributed fleet of Python workers coordinating over Redis (SRD §1) — that transport
doesn't exist yet, and `services/workers/` only has the contract stub for it. What's actually
wired into the orchestrator today is simpler — each `executors/<tool>/` directory is a real,
runnable project (a Playwright test suite, a Postman collection, a pytest suite, a Maestro flow)
that the Rust orchestrator invokes directly as a subprocess via `services/orchestrator/src/executor.rs`.
No distributed workers, no Redis in the loop — single-process, single-machine, matching the "MVP
= single-machine worker pool" reading from the PRD critique.

## Running the orchestrator

```
cd services/orchestrator
cargo run
```

Starts the API + WebSocket server on `http://localhost:8080`, backed by a SQLite file at
`infra/data/orchestrator.sqlite3` (created + migrated automatically on first run). **On a genuinely
first-ever run** (zero API keys in the DB), it mints a bootstrap admin key and prints it once to
stderr — copy it immediately, it is never shown again:

```
================================================================
AIQA bootstrap admin API key (shown once, save it now):

  aiqa_<64 hex chars>

Use it as: Authorization: Bearer aiqa_<...>
================================================================
```

Every route except `GET /health` requires `Authorization: Bearer <key>` (see auth.rs). Use the
bootstrap admin key to mint scoped keys for other roles/projects via `POST /admin/api-keys`.

Run the acceptance test suite (includes the SRD §25 continue-on-failure scenario, the
executor-param sanitizer regression tests, and the policy-engine tests — see Security below):

```
cargo test
```

## Routing a task to a real executor

A task with no `executor` field runs on the built-in `SimulatedExecutor` (`simulate: {outcome,
delay_ms}`). To run a real tool instead, set `executor` to one of `playwright` | `newman` |
`pytest` | `maestro` | `schemathesis` and pass its params:

| executor       | params                          | runs against                          |
|----------------|----------------------------------|----------------------------------------|
| `playwright`   | `spec` (default `tests/sample.spec.ts`), `grep` | `executors/playwright` |
| `newman`       | `folder` (default `health check passes`) | `executors/newman/collection.json` (targets this orchestrator's own `/health`) |
| `pytest`       | `node_id` (default `test_health.py`), `base_url` | `executors/pytest/test_health.py` |
| `maestro`      | `flow` (default `flow.yaml`)    | `executors/maestro/flow.yaml` (no device in this dev env — see its README) |
| `schemathesis` | `schema` (default `openapi.yaml`), `url` (default `http://localhost:8080`) | `executors/schemathesis/openapi.yaml` — property-based fuzzing, not example-based |

Example: `{"id":"t1","name":"...","executor":"playwright","params":{"grep":"homepage has correct title and heading"}}`

Some executors are gated by the policy engine (`policy.rs`, default: `maestro` requires approval —
tune with `AIQA_REQUIRE_APPROVAL_FOR`). A gated run comes back `PENDING_APPROVAL` instead of
`SCHEDULED`; release it with `POST /runs/:id/approve` or `POST /runs/:id/reject`.

## Security: executor params are sanitized, not just validated

`params` values reach the orchestrator via the public `POST /runs` body and get built into a shell
command line for the `playwright`/`newman`/`maestro` adapters (they're `.cmd` shims on Windows and
need `cmd /C` in front of them — see `shell_command` in `src/executor.rs`). Every param is passed
through `sanitize_token`, which rejects anything outside a conservative allowlist (letters, digits,
spaces, `. - _ / \ :`) and any `..` sequence, before it's ever built into that command line — this
is what stands between `POST /runs` and command injection. If you add a new param to an adapter,
route it through `safe_param`/`sanitize_token`; don't interpolate task input into a command string
directly.

## Running the dashboard

```
cd apps/web
npm install
npm run dev
```

Open `http://localhost:3000`, paste an API key into the field in the toolbar (the bootstrap admin
key printed by the orchestrator on its first run — see above), then click "Run demo suite" to
trigger the SRD §25 scenario (10 independent tasks where #2 and #7 fail, plus one dependent task)
against the orchestrator and watch it execute live. The key is kept in `localStorage` only.

## Infra (optional, for later persistence work)

```
cd infra
docker compose up -d
```

# AIQA Security Audit

## 1. Credential Security
- **P0 (historical finding)**: A Firebase API key was previously detected in `apps/web/firebase-applet-config.json`. The credential value has been redacted from this report and must not be committed to source control.
- **P2**: `localStorage` used for API key in `apps/web/services/apiClient.ts` (XSS risk)
- Backend uses Bearer token auth via `require_auth` middleware
- `.env.example` has safe placeholder values

## 2. Authentication & Authorization
- All project-scoped routes use `require_scoped_project` (auth + project isolation)
- Auth middleware in `services/orchestrator/src/auth.rs` validates Bearer tokens
- API key management via admin endpoints
- Settings page has no auth guard (**P3**)

## 3. Injection Protection
- Command injection: `services/orchestrator/src/executor.rs` uses `Command::new` with vector args, no shell concatenation
- SSRF: `services/orchestrator/src/execution_plan.rs` validates URLs against allowed domains
- Path traversal: Evidence uses UUID filenames, not user input
- SQL injection: `sqlx` parameterized queries throughout `services/orchestrator/src/store.rs`

## 4. CORS & Network
- `tower-http` CORS middleware configured (not permissive)
- Vite proxy for development
- WebSocket endpoints secured with auth

## 5. Process Isolation
- Windows Job Objects for subprocess execution (`services/orchestrator/src/executor.rs`)
- Prevents orphaned processes
- Has cancellation and timeout handling

## 6. Secret Scanning
- Secret scanning is enabled in CI.
- Reports and documentation must contain only redacted examples or placeholders, never live credentials.
- `.env.example` has safe placeholder values.

## 7. CI/CD Security
- CI/CD pipeline runs secret scanning, frontend validation, and backend validation.
- Add dependency vulnerability scanning as a future improvement.

## 8. Recommendations
- Revoke and rotate any credential that was previously committed.
- Keep credentials in GitHub Secrets or local environment files excluded from git.
- Consider `httpOnly` cookie for auth instead of `localStorage`.
- Add rate limiting.
- Add CSP headers.

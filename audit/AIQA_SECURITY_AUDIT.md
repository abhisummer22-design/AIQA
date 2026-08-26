# AIQA Security Audit

## 1. Credential Security
- **P0**: Firebase API key committed in [apps/web/firebase-applet-config.json](file:///G:/AIQA/apps/web/firebase-applet-config.json) (line 4: `AIzaSyDfYVz2EfWjFHlERFJtpeqnwCqjsb94guk`)
- **P2**: `localStorage` used for API key in [apps/web/services/apiClient.ts](file:///G:/AIQA/apps/web/services/apiClient.ts) (XSS risk)
- Backend uses Bearer token auth via `require_auth` middleware
- `.env.example` has safe placeholder values

## 2. Authentication & Authorization
- All project-scoped routes use `require_scoped_project` (auth + project isolation)
- Auth middleware in [services/orchestrator/src/auth.rs](file:///G:/AIQA/services/orchestrator/src/auth.rs) validates Bearer tokens
- API key management via admin endpoints
- Settings page has no auth guard (**P3**)

## 3. Injection Protection
- Command injection: [services/orchestrator/src/executor.rs](file:///G:/AIQA/services/orchestrator/src/executor.rs) uses `Command::new` with vector args, no shell concatenation
- SSRF: [services/orchestrator/src/execution_plan.rs](file:///G:/AIQA/services/orchestrator/src/execution_plan.rs) validates URLs against allowed domains
- Path traversal: Evidence uses UUID filenames, not user input
- SQL injection: `sqlx` parameterized queries throughout [services/orchestrator/src/store.rs](file:///G:/AIQA/services/orchestrator/src/store.rs)

## 4. CORS & Network
- `tower-http` CORS middleware configured (not permissive)
- Vite proxy for development
- WebSocket endpoints secured with auth

## 5. Process Isolation
- Windows Job Objects for subprocess execution ([services/orchestrator/src/executor.rs](file:///G:/AIQA/services/orchestrator/src/executor.rs))
- Prevents orphaned processes
- Has cancellation and timeout handling

## 6. Secret Scanning
- Firebase API key is the ONLY committed secret
- No secrets in git history
- `.env.example` has safe placeholders
- Test files use fake keys (`sk-real-looking-value`) that are not real

## 7. CI/CD Security
- **P1**: No CI/CD pipeline exists - no automated security scanning
- No dependency vulnerability scanning
- No secret detection in commits

## 8. Recommendations
- Immediately revoke Firebase API key
- Add CI/CD with security scanning
- Consider `httpOnly` cookie for auth instead of `localStorage`
- Add rate limiting
- Add CSP headers

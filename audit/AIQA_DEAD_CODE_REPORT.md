# AIQA Dead Code Report

## Dead Files
- **apps/web/** (43 files) - Duplicate legacy frontend. Evidence: start.bat launches frontend/, not apps/web. Safe to delete: Requires Human Review.
- **worker stubs** (4 .py files in services/workers/) - Stub python adapters throwing NotImplementedError. Safe to delete: Requires Human Review.

## Dead Functions
- **activities state**: AIQAContext.tsx:223 - Zero consumers in codebase.
- **addActivity**: AIQAContext.tsx - Exported but never called.

## Dead Routes
- **Legacy flat routes**: /runs, /admin/*, /intelligence/*, /bug-drafts - Replaced by scoped routes. Safe to delete: Yes (after verifying consumers).

## Dead Dependencies
- **express**: in frontend package.json - Questionable inclusion. Safe to delete: Yes.

## Fabricated Data
- **activeGenerationSession**: AIQAContext.tsx:154-212 - Hardcoded generation cases. Safe to delete: Needs refactoring to use real data.

## Dead Config
- **docker-compose.yml**: infra/ - Postgres/Redis/MinIO defined but orchestrator uses SQLite. Safe to delete: Requires Human Review (might be for future).

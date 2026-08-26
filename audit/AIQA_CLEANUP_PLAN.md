# AIQA Phased Cleanup Plan

## Phase A: Immediate Fixes
- Revoke Firebase key in irebase-applet-config.json.
- Add GitHub Actions CI/CD pipeline.
- Remove fabricated ctiveGenerationSession and dead ctivities from AIQAContext.tsx.

## Phase B: Human Review Items
- Decide whether to keep pps/web/ (Google AI Studio frontend) or delete.
- Evaluate legacy flat routes for full removal.
- Evaluate infra/docker-compose.yml.

## Phase C: Architecture Improvements
- Implement httpOnly cookies instead of localStorage for API keys.
- Add lazy loading for route components.
- Add missing component tests.

## Phase D: Fix Broken Wiring
- None found - The backend implementations use real API routing, AI providers, and sub-processes. The codebase is truthful.

## Phase E: Implement Partial Features
- MCP architecture implementation.
- Build deferred pages (Orchestrator, Validator, RBAC, Mobile, FeatureMap).

## Phase F: E2E Verification
- Add E2E tests for main execution and generation flows.

## Phase G: Final Re-Audit
- Re-audit the repository to ensure compliance.

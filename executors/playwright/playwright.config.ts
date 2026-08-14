import { defineConfig } from "@playwright/test";

// Minimal config for a single headless Chromium project. The orchestrator
// invokes `npx playwright test <spec> --grep <title>` per task, so no
// sharding/retries config is needed here — AIQA's own scheduler owns
// concurrency and retries (FRD §5, §21).
export default defineConfig({
  testDir: "./tests",
  reporter: "line",
  use: {
    headless: true,
  },
});

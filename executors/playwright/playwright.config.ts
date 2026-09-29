import { defineConfig } from "@playwright/test";

// Minimal config for a single Chromium project. The orchestrator invokes
// `npx playwright test <spec> --grep <title>` per task, so no sharding/
// retries config is needed here — AIQA's own scheduler owns concurrency and
// retries (FRD §5, §21).
//
// Headless by default (the only behavior every earlier release had).
// `executor.rs::PlaywrightExecutor` sets AIQA_PLAYWRIGHT_HEADED=1 in the
// spawned process's real environment when Settings -> Test Execution ->
// "Show the browser window while tests run" is on (`GET`/`PUT
// /api/v1/execution-settings`), so a real, visible Chromium window opens for
// that run instead. Any other value (unset, "0", …) keeps the old default.
export default defineConfig({
  testDir: "./tests",
  reporter: "line",
  use: {
    headless: process.env.AIQA_PLAYWRIGHT_HEADED !== "1",
  },
});

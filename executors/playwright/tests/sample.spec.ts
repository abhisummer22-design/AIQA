import { test, expect } from "@playwright/test";

// Self-contained fixture: loads inline HTML via a data: URL so this executor
// has no dependency on any external app or running server. Real assertions,
// real browser, real pass/fail — just no product-under-test wired up yet.
const FIXTURE_HTML =
  "<title>AIQA Fixture</title><h1>Hello AIQA</h1><button id=go>Click me</button>" +
  "<script>document.getElementById('go').onclick=()=>{document.body.dataset.clicked='true'}</script>";

test("homepage has correct title and heading", async ({ page }) => {
  await page.goto(`data:text/html,${encodeURIComponent(FIXTURE_HTML)}`);
  await expect(page).toHaveTitle("AIQA Fixture");
  await expect(page.locator("h1")).toHaveText("Hello AIQA");
});

test("button click updates page state", async ({ page }) => {
  await page.goto(`data:text/html,${encodeURIComponent(FIXTURE_HTML)}`);
  await page.locator("#go").click();
  await expect(page.locator("body")).toHaveAttribute("data-clicked", "true");
});

// Deliberately wrong assertion, kept in the suite so the demo run has a real
// (not simulated) Playwright failure to show off FRD §2's FAILED status.
test("deliberately broken assertion", async ({ page }) => {
  await page.goto(`data:text/html,${encodeURIComponent(FIXTURE_HTML)}`);
  await expect(page.locator("h1")).toHaveText("This text does not appear on the page");
});

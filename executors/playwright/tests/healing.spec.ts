import { test, expect } from "@playwright/test";
import { healingLocator } from "../helpers/healing";

// Self-contained fixture, same pattern sample.spec.ts already uses: inline
// HTML via a data: URL, no dependency on any external app. Real browser,
// real DOM, real pass/fail — the point of these tests is to prove
// `healingLocator` genuinely finds a real element via a real fallback
// strategy (not merely "does not crash"), and genuinely fails a real test
// when nothing can be found.
const FIXTURE_HTML =
  "<title>AIQA Healing Fixture</title>" +
  '<button id="real-submit-button" data-testid="submit-btn">Submit Form</button>' +
  '<input id="real-email-input" name="email" placeholder="Email address" />' +
  "<script>" +
  "document.getElementById('real-submit-button').onclick = () => { document.body.dataset.submitted = 'true'; };" +
  "</script>";

test.beforeEach(async ({ page }) => {
  await page.goto(`data:text/html,${encodeURIComponent(FIXTURE_HTML)}`);
});

test("primary selector that still matches is used directly, no healing", async ({
  page,
}) => {
  const locator = await healingLocator(page, "#real-submit-button", 0, "CLICK");
  await locator.click();
  await expect(page.locator("body")).toHaveAttribute("data-submitted", "true");
});

test("a stale id selector heals via a real data-testid embedded in the same selector string", async ({
  page,
}) => {
  // The id half of this compound selector matches nothing on the fixture
  // page — only the real, embedded data-testid half does. A real heal, via
  // Playwright's own getByTestId, must still resolve the real button.
  const locator = await healingLocator(
    page,
    '#stale-button-id[data-testid="submit-btn"]',
    0,
    "CLICK"
  );
  await locator.click();
  await expect(page.locator("body")).toHaveAttribute("data-submitted", "true");
});

test("a stale id selector heals via a real name-derived placeholder match for a fill target", async ({
  page,
}) => {
  const locator = await healingLocator(
    page,
    '#stale-email-id[name="email"]',
    1,
    "FILL"
  );
  await locator.fill("user@example.com");
  await expect(page.locator("#real-email-input")).toHaveValue(
    "user@example.com"
  );
});

test("a selector with no real basis for any fallback is a real, honest failure", async ({
  page,
}) => {
  await expect(
    healingLocator(
      page,
      "#totally-wrong-nonexistent-element-xyz",
      2,
      "CLICK"
    )
  ).rejects.toThrow(/self-healing exhausted every real fallback strategy/);
});

test("a genuinely hidden element found by its real primary selector still fails its own visibility assertion, healing must not mask it", async ({
  page,
}) => {
  // A real element that exists but is legitimately hidden must still fail
  // ASSERT_VISIBLE on its own merits — healing (which only ever triggers on
  // a complete, zero-match miss, never on "exists but hidden") must not
  // swap to a different, visible element just to force this to pass. See
  // helpers/healing.ts's own top-of-file doc comment for why.
  await page.evaluate(() => {
    const hidden = document.createElement("button");
    hidden.id = "hidden-button";
    hidden.textContent = "Hidden Button";
    hidden.style.display = "none";
    document.body.appendChild(hidden);
  });
  const locator = await healingLocator(page, "#hidden-button", 3, "ASSERT_VISIBLE");
  await expect(locator).not.toBeVisible();
});

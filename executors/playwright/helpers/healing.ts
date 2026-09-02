import { test, Locator, Page } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";

/**
 * Self-healing locator resolution.
 *
 * `materialize_playwright_spec`
 * (services/orchestrator/src/executor.rs) compiles every CLICK / FILL /
 * ASSERT_VISIBLE / ASSERT_TEXT action through this function instead of a
 * raw `page.locator(selector)` call. The primary selector is always tried
 * first (fast path — correct most of the time); only when it matches
 * nothing at all does this reach for a small, real set of alternative
 * Playwright locator strategies — every one of them derived from the
 * primary selector's own embedded semantics (its id / class / attribute
 * names and values), never a blind, ungrounded guess.
 *
 * Safety rule (important — read before changing the trigger condition):
 * healing only ever triggers when the primary selector matches ZERO
 * elements ("attached" check below, not "visible"). Whether the element a
 * locator resolves to then satisfies THIS action's own real requirement —
 * visible for a click, visible-and-text-matching for ASSERT_TEXT, etc. —
 * is left entirely to that action's own native Playwright wait/assert
 * semantics afterward, unmodified. A correctly-found element that is
 * legitimately hidden must still fail its own assertion on its own
 * merits; healing must never swap to a different, visible element just to
 * force that assertion to pass. Self-healing adapts to a stale selector,
 * it never fabricates a pass.
 *
 * Heal succeeded -> the resolved Locator is returned, and a real event is
 * recorded — both via `console.log` (so it lands in this run's own
 * captured stdout with zero extra plumbing; see
 * `executor.rs::outcome_from_output`, which already folds full
 * stdout+stderr into `ExecutionOutcome.evidence`) and appended to a
 * JSON-lines artifact file for machine consumption.
 * Heal failed (no strategy resolved anything) -> this throws a real,
 * honest error immediately, with the full healing story already in the
 * message, rather than letting the caller hit a less specific timeout.
 */

const ATTACHED_CHECK_TIMEOUT_MS = 1500;

interface HealingEvent {
  event: "heal";
  actionIndex: number;
  actionKind: string;
  primarySelector: string;
  strategy: string;
  resolved: boolean;
}

interface Candidate {
  strategy: string;
  build: (page: Page) => Locator;
}

/** "submit-btn" / "submit_btn" / "submitBtn" -> "submit btn" */
function humanize(raw: string): string {
  return raw
    .replace(/[-_]+/g, " ")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .trim();
}

/**
 * Every candidate here is built from a real substring of `selector`
 * itself — an id, a class, or an attribute name/value the primary
 * selector already contains. This is re-deriving a different, more
 * resilient Playwright API call for the SAME element the primary selector
 * already named, never inventing new information about the page.
 */
function deriveFallbackCandidates(selector: string): Candidate[] {
  const candidates: Candidate[] = [];

  const testId = selector.match(/\[data-test(?:-?id)?="([^"]+)"\]/i);
  if (testId) {
    const value = testId[1];
    candidates.push({
      strategy: "testid",
      build: (page) => page.getByTestId(value),
    });
  }

  const ariaLabel = selector.match(/\[aria-label="([^"]+)"\]/i);
  if (ariaLabel) {
    const value = ariaLabel[1];
    candidates.push({
      strategy: "label",
      build: (page) => page.getByLabel(value),
    });
    candidates.push({
      strategy: "aria-name",
      build: (page) => page.getByRole("button", { name: value }),
    });
  }

  const placeholder = selector.match(/\[placeholder="([^"]+)"\]/i);
  if (placeholder) {
    const value = placeholder[1];
    candidates.push({
      strategy: "placeholder",
      build: (page) => page.getByPlaceholder(value),
    });
  }

  const nameAttr = selector.match(/\[name="([^"]+)"\]/i);
  if (nameAttr) {
    const value = humanize(nameAttr[1]);
    if (value.length > 0) {
      candidates.push({
        strategy: "label-from-name",
        build: (page) => page.getByLabel(new RegExp(value, "i")),
      });
      candidates.push({
        strategy: "placeholder-from-name",
        build: (page) => page.getByPlaceholder(new RegExp(value, "i")),
      });
    }
  }

  const idMatch = selector.match(/#([a-zA-Z][\w-]*)/);
  if (idMatch) {
    const value = humanize(idMatch[1]);
    if (value.length > 0) {
      candidates.push({
        strategy: "text-from-id",
        build: (page) => page.getByText(new RegExp(value, "i")),
      });
      candidates.push({
        strategy: "role-from-id",
        build: (page) =>
          page.getByRole("button", { name: new RegExp(value, "i") }),
      });
    }
  }

  const classMatch = selector.match(/\.([a-zA-Z][\w-]*)/);
  if (classMatch) {
    const value = humanize(classMatch[1]);
    if (value.length > 0) {
      candidates.push({
        strategy: "text-from-class",
        build: (page) => page.getByText(new RegExp(value, "i")),
      });
    }
  }

  // Structural/sibling relaxation: a compound selector's last simple part
  // alone, tolerant of an ancestor's markup having shifted around it.
  const parts = selector.trim().split(/\s+/);
  if (parts.length > 1) {
    const lastPart = parts[parts.length - 1];
    if (lastPart && lastPart !== selector) {
      candidates.push({
        strategy: "relaxed-structural",
        build: (page) => page.locator(lastPart).first(),
      });
    }
  }

  return candidates;
}

/**
 * "Does at least one real element matching this locator exist in the DOM
 * right now" — deliberately `state: "attached"`, not `"visible"`. See this
 * file's own top-of-file doc comment for why that distinction is the whole
 * safety rule this module rests on.
 */
async function isAttached(locator: Locator): Promise<boolean> {
  try {
    await locator.first().waitFor({
      state: "attached",
      timeout: ATTACHED_CHECK_TIMEOUT_MS,
    });
    return true;
  } catch {
    return false;
  }
}

function eventsFilePath(): string {
  const info = test.info();
  const safeId = info.testId.replace(/[^a-zA-Z0-9_-]/g, "_");
  return path.join(
    __dirname,
    "..",
    "tests",
    "generated",
    `healing-events-${safeId}.jsonl`
  );
}

function recordEvent(ev: HealingEvent): void {
  // The real event, visible in this run's own captured evidence with zero
  // extra plumbing on the Rust side (see this file's top-of-file comment).
  console.log(`AIQA_HEALING_EVENT ${JSON.stringify(ev)}`);
  try {
    const filePath = eventsFilePath();
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.appendFileSync(filePath, JSON.stringify(ev) + "\n");
  } catch (e) {
    // A failure to write the durability artifact must never break the
    // test itself — the console.log line above already carries the same
    // real event into this run's captured evidence.
    console.log(`AIQA_HEALING_EVENT_WRITE_FAILED ${String(e)}`);
  }
}

export async function healingLocator(
  page: Page,
  primarySelector: string,
  actionIndex: number,
  actionKind: string
): Promise<Locator> {
  const primary = page.locator(primarySelector);
  if (await isAttached(primary)) {
    return primary;
  }

  for (const candidate of deriveFallbackCandidates(primarySelector)) {
    const resolved = candidate.build(page);
    if (await isAttached(resolved)) {
      recordEvent({
        event: "heal",
        actionIndex,
        actionKind,
        primarySelector,
        strategy: candidate.strategy,
        resolved: true,
      });
      return resolved;
    }
  }

  recordEvent({
    event: "heal",
    actionIndex,
    actionKind,
    primarySelector,
    strategy: "none",
    resolved: false,
  });
  throw new Error(
    `AIQA self-healing exhausted every real fallback strategy for selector ${JSON.stringify(
      primarySelector
    )} (action #${actionIndex}, ${actionKind}) — no matching element found.`
  );
}

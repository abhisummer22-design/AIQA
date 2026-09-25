import { Browser } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";
import { resolveValue, scrub } from "./secrets";

/**
 * Real crawler + per-role visibility matrix (Phase 6). Mirrors
 * `services/orchestrator/src/execution_plan.rs`'s `Action` enum exactly —
 * `type: "NAVIGATE" | "FILL" | "CLICK"`, same field names — so a real,
 * caller-supplied login sequence is described with the same closed, safe
 * vocabulary a compiled test case's browser actions already use, rather
 * than a second one invented for this purpose. Only these three action
 * kinds make sense mid-login; the Rust side's own `validate_crawl_plan`
 * rejects anything else before this ever runs.
 */
export interface CrawlAction {
  type: "NAVIGATE" | "FILL" | "CLICK";
  url?: string;
  selector?: string;
  /** For FILL: the text to type, or `env:NAME` to read it from the environment variable `NAME` (an
   * `AIQA_LOGIN_*` / `AIQA_SECRET_*` name) when the crawl runs — see `helpers/secrets.ts`. */
  value?: string;
}

export interface CrawlRoleConfig {
  name: string;
  /** Empty = crawled anonymously; no login attempted. */
  loginActions: CrawlAction[];
}

export interface CrawlPlan {
  baseUrl: string;
  roles: CrawlRoleConfig[];
  /** Words (stems of what the requirements are about) to look for first: a link whose text or address mentions one is
   * followed before the others, and may take a page beyond the usual limit. Absent or empty: the crawl is what it always was. */
  focusTerms?: string[];
}

export interface CrawledElement {
  kind: "link" | "button";
  text: string;
  /** Best-effort real selector for this exact element — same derivation
   * spirit `helpers/healing.ts` already uses for fallback candidates
   * (prefer a real testid/id, fall back to tag name). */
  selectorHint: string;
  /** Only present for links — the real, browser-resolved absolute URL
   * (not the raw `href` attribute, which may be relative). */
  href?: string;
}

export interface CrawledPage {
  url: string;
  title: string;
  elements: CrawledElement[];
}

export interface RoleCrawlResult {
  role: string;
  /** `false` when the real login sequence itself failed (a selector in
   * `loginActions` never resolved, or threw) — the rest of this role's
   * crawl is whatever pages were reachable before that point, honestly
   * reported, never padded to look like a successful crawl. */
  loginSucceeded: boolean;
  loginError?: string;
  pages: CrawledPage[];
}

export interface VisibilityDiffEntry {
  url: string;
  /** Every role that could reach this URL at all during its own crawl. */
  rolesWithAccess: string[];
  /** Per role, element text genuinely seen by that role's crawl and by NO
   * other role's crawl of the same URL — the real "only this role sees
   * this" signal, computed from actual crawl data, never guessed. */
  elementsOnlyVisibleTo: Record<string, string[]>;
}

export interface CrawlOutput {
  roles: RoleCrawlResult[];
  visibilityMatrix: VisibilityDiffEntry[];
}

const MAX_PAGES_PER_ROLE = 20;
/** With focus terms, this many more pages than `MAX_PAGES_PER_ROLE` may be visited, if they are ones a focus term led to. */
export const MAX_FOCUS_EXTRA_PAGES = 10;
const NAV_TIMEOUT_MS = 8000;

function sameOrigin(a: string, b: string): boolean {
  try {
    return new URL(a).origin === new URL(b).origin;
  } catch {
    return false;
  }
}

/** `secretsUsed` collects the values that came from `env:NAME` references, so an error's message can be scrubbed of
 * them before it is written to the result. */
async function runLoginActions(
  page: import("@playwright/test").Page,
  actions: CrawlAction[],
  secretsUsed: string[]
): Promise<void> {
  for (const action of actions) {
    switch (action.type) {
      case "NAVIGATE":
        await page.goto(action.url!, { timeout: NAV_TIMEOUT_MS });
        break;
      case "FILL": {
        // A value written `env:NAME` is read from the environment now, never stored in the plan or the spec.
        const value = resolveValue(action.value!);
        if (value !== action.value) secretsUsed.push(value);
        await page.locator(action.selector!).fill(value, { timeout: NAV_TIMEOUT_MS });
        break;
      }
      case "CLICK":
        await page.locator(action.selector!).click({ timeout: NAV_TIMEOUT_MS });
        break;
    }
  }
}

async function extractPageElements(
  page: import("@playwright/test").Page
): Promise<CrawledElement[]> {
  return page.evaluate(() => {
    const describe = (el: Element): string => {
      const testid = el.getAttribute("data-testid");
      if (testid) return `[data-testid="${testid}"]`;
      if (el.id) return `#${el.id}`;
      const cls = el.getAttribute("class");
      if (cls) return `.${cls.trim().split(/\s+/)[0]}`;
      return el.tagName.toLowerCase();
    };
    const out: { kind: "link" | "button"; text: string; selectorHint: string; href?: string }[] = [];
    document.querySelectorAll("a[href]").forEach((el) => {
      out.push({
        kind: "link",
        text: (el.textContent || "").trim(),
        selectorHint: describe(el),
        href: (el as HTMLAnchorElement).href,
      });
    });
    document.querySelectorAll("button, input[type=submit]").forEach((el) => {
      const text =
        (el.textContent || "").trim() || (el as HTMLInputElement).value || "";
      out.push({ kind: "button", text, selectorHint: describe(el) });
    });
    return out;
  });
}

// ---- Phase 5 grounding: what the crawl records for compiling test cases on ---------------------------------------------
//
// Alongside its result, a crawl writes `crawl-grounding-<id>.json` (`services/orchestrator/src/grounding.rs` reads it).
// The result (`CrawlOutput`) is unchanged: links and buttons, with a best-effort `selectorHint`. The grounding lists
// what a test can act on — links, buttons and form fields — each with a locator this crawl CHECKED matches exactly one
// element on its page, and each role's login steps with every credential replaced by its `env:NAME` reference (a
// literal credential is never kept: `loginUsesLiterals` says the role cannot be logged in again by a test).

export type GroundedKind = "link" | "button" | "input" | "select" | "textarea";

export interface GroundedElement {
  kind: GroundedKind;
  /** What a person would call it: a link's or button's text, a field's label, placeholder or name. */
  text: string;
  /** A Playwright selector string that matched exactly one element on this page when it was crawled. */
  locator: string;
  href?: string;
  /** For an `input`: its `type` (`text`, `password`, ...). */
  inputType?: string;
}

export interface GroundedPage {
  url: string;
  title: string;
  elements: GroundedElement[];
  /** The page's `h1`-`h3` headings (what it says it is about), when it has any. */
  headings?: string[];
}

export interface GroundedRole {
  role: string;
  loginSucceeded: boolean;
  loginError?: string;
  loginUsesLiterals: boolean;
  loginActions: CrawlAction[];
  pages: GroundedPage[];
}

export interface CrawlGrounding {
  roles: GroundedRole[];
}

const MAX_GROUNDED_ELEMENTS_PER_PAGE = 80;

interface RawGroundedElement {
  kind: GroundedKind;
  text: string;
  href?: string;
  inputType?: string;
  /** CSS locators already checked in the page to match exactly this one element, best first. */
  candidates: string[];
  /** For the accessible-name locator tried when no CSS one is unique. */
  role: string;
  roleName: string;
}

/** Every visible link, button and form field of the page, with a locator that matches exactly it. */
export async function extractGroundedElements(
  page: import("@playwright/test").Page
): Promise<GroundedElement[]> {
  const raw: RawGroundedElement[] = await page.evaluate(() => {
    const quote = (v: string) => v.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
    const squash = (v: string | null | undefined) => (v || "").replace(/\s+/g, " ").trim();
    const isVisible = (el: Element): boolean => {
      const box = (el as HTMLElement).getClientRects();
      if (box.length === 0) return false;
      const style = getComputedStyle(el as HTMLElement);
      return style.visibility !== "hidden" && style.display !== "none";
    };
    const labelOf = (el: Element): string => {
      const aria = squash(el.getAttribute("aria-label"));
      if (aria) return aria;
      const id = el.getAttribute("id");
      if (id) {
        const label = document.querySelector(`label[for="${quote(id)}"]`);
        if (label) return squash(label.textContent);
      }
      const wrapping = el.closest("label");
      if (wrapping) return squash(wrapping.textContent);
      return squash(el.getAttribute("placeholder")) || squash(el.getAttribute("name")) || squash(id);
    };
    const candidatesFor = (el: Element): string[] => {
      const tag = el.tagName.toLowerCase();
      const all: string[] = [];
      for (const attr of ["data-testid", "data-test", "data-qa", "data-cy"]) {
        const v = el.getAttribute(attr);
        if (v) all.push(`[${attr}="${quote(v)}"]`);
      }
      const id = el.getAttribute("id");
      if (id) all.push(/^[A-Za-z][\w-]*$/.test(id) ? `#${id}` : `[id="${quote(id)}"]`);
      const name = el.getAttribute("name");
      if (name) all.push(`${tag}[name="${quote(name)}"]`);
      const aria = el.getAttribute("aria-label");
      if (aria) all.push(`${tag}[aria-label="${quote(aria)}"]`);
      const placeholder = el.getAttribute("placeholder");
      if (placeholder) all.push(`${tag}[placeholder="${quote(placeholder)}"]`);
      if (tag === "a") {
        const href = el.getAttribute("href");
        if (href) all.push(`a[href="${quote(href)}"]`);
      }
      return all.filter((css) => {
        try {
          const found = document.querySelectorAll(css);
          return found.length === 1 && found[0] === el;
        } catch {
          return false;
        }
      });
    };

    const out: RawGroundedElement[] = [];
    document.querySelectorAll("a[href], button, input, select, textarea").forEach((el) => {
      if (!isVisible(el)) return;
      const tag = el.tagName.toLowerCase();
      let kind: GroundedKind;
      let text: string;
      let role: string;
      let inputType: string | undefined;
      if (tag === "a") {
        kind = "link";
        text = squash(el.textContent) || squash(el.getAttribute("aria-label")) || squash(el.getAttribute("title"));
        role = "link";
      } else if (tag === "button") {
        kind = "button";
        text = squash(el.textContent) || squash(el.getAttribute("aria-label"));
        role = "button";
      } else if (tag === "select") {
        kind = "select";
        text = labelOf(el);
        role = "combobox";
      } else if (tag === "textarea") {
        kind = "textarea";
        text = labelOf(el);
        role = "textbox";
      } else {
        const type = (el.getAttribute("type") || "text").toLowerCase();
        if (type === "hidden") return;
        if (["submit", "button", "reset", "image"].includes(type)) {
          kind = "button";
          text = squash((el as HTMLInputElement).value) || labelOf(el);
          role = "button";
        } else {
          kind = "input";
          inputType = type;
          text = labelOf(el);
          role = type === "checkbox" ? "checkbox" : type === "radio" ? "radio" : "textbox";
        }
      }
      out.push({
        kind,
        text,
        href: tag === "a" ? (el as HTMLAnchorElement).href : undefined,
        inputType,
        candidates: candidatesFor(el),
        role,
        roleName: text,
      });
    });
    return out;
  });

  const grounded: GroundedElement[] = [];
  const seen = new Set<string>();
  for (const el of raw) {
    let locator: string | undefined = el.candidates[0];
    if (!locator && el.roleName) {
      // No CSS locator names this element alone: try its accessible name, and keep it only if it matches one element.
      const byRole = `role=${el.role}[name=${JSON.stringify(el.roleName)}]`;
      try {
        if ((await page.locator(byRole).count()) === 1) locator = byRole;
      } catch {
        /* not a usable locator — the element is simply not offered */
      }
    }
    if (!locator || seen.has(locator)) continue;
    seen.add(locator);
    grounded.push({ kind: el.kind, text: el.text, locator, href: el.href, inputType: el.inputType });
    if (grounded.length >= MAX_GROUNDED_ELEMENTS_PER_PAGE) break;
  }
  return grounded;
}

/** A role's login steps as they are kept: every `env:NAME` reference as it was written, every literal value removed. */
function keptLogin(actions: CrawlAction[]): { actions: CrawlAction[]; usesLiterals: boolean } {
  let usesLiterals = false;
  const kept = actions.map((action) => {
    if (action.type !== "FILL") return action;
    if (typeof action.value === "string" && action.value.startsWith("env:")) return action;
    usesLiterals = true;
    return { ...action, value: "" };
  });
  return { actions: kept, usesLiterals };
}

/** `crawl-result-<id>.json` -> `crawl-grounding-<id>.json` (the name `grounding.rs` looks for). */
export function groundingFileName(resultFileName: string): string {
  return resultFileName.startsWith("crawl-result-")
    ? resultFileName.replace("crawl-result-", "crawl-grounding-")
    : `grounding-${resultFileName}`;
}

// ---- Requirement-driven crawling: which links to follow first -----------------------------------------------------------

/** How many of `terms` a link mentions, in its text or in the path of its address. */
export function focusScore(text: string, href: string, terms: string[]): number {
  if (terms.length === 0) return 0;
  let path = "";
  try {
    path = new URL(href).pathname;
  } catch {
    path = href;
  }
  const haystack = `${text} ${path}`.toLowerCase();
  return terms.filter((t) => t.length > 0 && haystack.includes(t.toLowerCase())).length;
}

/** Puts newly found links in their queues: those that mention a focus term in `focusQueue`, most terms first (a link found
 * earlier stays ahead of an equal one found later), the rest at the end of `queue`. */
export function enqueueLinks(
  queue: string[],
  focusQueue: string[],
  links: { text: string; href: string }[],
  terms: string[]
): void {
  const scores = new Map<string, number>();
  for (const url of focusQueue) scores.set(url, focusScore("", url, terms));
  for (const link of links) {
    const score = focusScore(link.text, link.href, terms);
    if (score === 0) {
      queue.push(link.href);
      continue;
    }
    if (scores.has(link.href)) continue;
    scores.set(link.href, score);
    let at = focusQueue.length;
    while (at > 0 && (scores.get(focusQueue[at - 1]) ?? 0) < score) at -= 1;
    focusQueue.splice(at, 0, link.href);
  }
}

/** The next address to visit, or `undefined` when the crawl is done: a focus link while the page budget (the usual limit
 * plus the extra a focused crawl gets) has room, otherwise the next ordinary link while the usual limit has room. */
export function takeNext(
  queue: string[],
  focusQueue: string[],
  pagesSoFar: number,
  focusing: boolean
): string | undefined {
  if (focusQueue.length > 0 && pagesSoFar < MAX_PAGES_PER_ROLE + (focusing ? MAX_FOCUS_EXTRA_PAGES : 0)) {
    return focusQueue.shift();
  }
  if (queue.length > 0 && pagesSoFar < MAX_PAGES_PER_ROLE) return queue.shift();
  return undefined;
}

/** The visible `h1`-`h3` headings of the page: what it says it is about. */
export async function extractHeadings(page: import("@playwright/test").Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    document.querySelectorAll('h1, h2, h3, [role="heading"]').forEach((el) => {
      const box = (el as HTMLElement).getClientRects();
      if (box.length === 0) return;
      const style = getComputedStyle(el as HTMLElement);
      if (style.visibility === "hidden" || style.display === "none") return;
      const text = (el.textContent || "").replace(/\s+/g, " ").trim().slice(0, 80);
      if (text && !out.includes(text)) out.push(text);
    });
    return out.slice(0, 12);
  });
}

/** One role's real crawl: real login (if configured), then a real,
 * bounded, same-origin breadth-first crawl from `baseUrl`, recording every
 * page's real title and real interactive elements. Never invents a page
 * this role's own session didn't actually reach. */
async function crawlAsRole(
  browser: Browser,
  baseUrl: string,
  role: CrawlRoleConfig,
  groundedPages: GroundedPage[],
  focusTerms: string[] = []
): Promise<RoleCrawlResult> {
  const context = await browser.newContext();
  const page = await context.newPage();

  let loginSucceeded = true;
  let loginError: string | undefined;
  const secretsUsed: string[] = [];
  try {
    await runLoginActions(page, role.loginActions, secretsUsed);
  } catch (e) {
    loginSucceeded = false;
    loginError = scrub(String(e), secretsUsed);
  }

  const visited = new Set<string>();
  const queue: string[] = [baseUrl];
  const focusQueue: string[] = [];
  const focusing = focusTerms.length > 0;
  const pages: CrawledPage[] = [];

  for (;;) {
    const url = takeNext(queue, focusQueue, pages.length, focusing);
    if (url === undefined) break;
    if (visited.has(url)) continue;
    visited.add(url);

    try {
      await page.goto(url, { waitUntil: "domcontentloaded", timeout: NAV_TIMEOUT_MS });
    } catch {
      continue; // a real, unreachable page for this role — skipped, not fabricated
    }
    const title = await page.title();
    const elements = await extractPageElements(page);
    const finalUrl = page.url();
    // A redirect (e.g. `/` -> `/dashboard`) means the URL actually reached
    // differs from the one dequeued — mark it visited too, or the same
    // real destination gets queued and loaded again the first time some
    // other page links to it directly.
    visited.add(finalUrl);
    pages.push({ url: finalUrl, title, elements });
    // What a test can act on, for compiling test cases on this page (see the grounding types above). A page whose
    // grounding cannot be read is simply not offered for it; the crawl result above is unaffected.
    try {
      const groundedElements = await extractGroundedElements(page);
      let headings: string[] = [];
      try {
        headings = await extractHeadings(page);
      } catch {
        /* a page whose headings cannot be read is offered without them */
      }
      groundedPages.push({ url: finalUrl, title, elements: groundedElements, ...(headings.length > 0 ? { headings } : {}) });
    } catch {
      /* not offered for grounding */
    }

    enqueueLinks(
      queue,
      focusQueue,
      elements
        .filter((el) => el.kind === "link" && el.href && sameOrigin(el.href, baseUrl) && !visited.has(el.href))
        .map((el) => ({ text: el.text, href: el.href as string })),
      focusTerms
    );
  }

  await context.close();
  return { role: role.name, loginSucceeded, loginError, pages };
}

/** The real diff — for every URL any role reached, which roles reached it
 * at all, and per role, which element text was genuinely seen by that
 * role's crawl and no other's. Pure function of the real crawl results
 * above; no page is visited again here. */
export function computeVisibilityMatrix(results: RoleCrawlResult[]): VisibilityDiffEntry[] {
  const allUrls = new Set<string>();
  for (const r of results) for (const p of r.pages) allUrls.add(p.url);

  const diff: VisibilityDiffEntry[] = [];
  for (const url of [...allUrls].sort()) {
    const rolesWithAccess: string[] = [];
    const textsByRole: Record<string, Set<string>> = {};
    for (const r of results) {
      const p = r.pages.find((pg) => pg.url === url);
      if (p) {
        rolesWithAccess.push(r.role);
        textsByRole[r.role] = new Set(p.elements.map((e) => e.text).filter((t) => t.length > 0));
      }
    }
    const elementsOnlyVisibleTo: Record<string, string[]> = {};
    for (const [role, texts] of Object.entries(textsByRole)) {
      const onlyThisRole = [...texts]
        .filter((t) =>
          Object.entries(textsByRole).every(([otherRole, otherTexts]) => otherRole === role || !otherTexts.has(t))
        )
        .sort();
      if (onlyThisRole.length > 0) elementsOnlyVisibleTo[role] = onlyThisRole;
    }
    diff.push({ url, rolesWithAccess: rolesWithAccess.sort(), elementsOnlyVisibleTo });
  }
  return diff;
}

/** The real entry point a materialized crawl spec calls. Crawls every
 * configured role in turn (sequentially — each gets its own fresh browser
 * context, so one role's session can never leak into another's), computes
 * the real visibility matrix, then records the result twice: a compact
 * summary via `console.log` (so it lands in this run's own captured
 * evidence with zero extra Rust-side plumbing — same pattern
 * `helpers/healing.ts` already established) and the full structured
 * output to `resultFileName` for `crawler::read_crawl_result` to parse. */
export async function runCrawl(
  browser: Browser,
  plan: CrawlPlan,
  resultFileName: string
): Promise<void> {
  const roles: RoleCrawlResult[] = [];
  const groundedRoles: GroundedRole[] = [];
  for (const role of plan.roles) {
    const groundedPages: GroundedPage[] = [];
    const result = await crawlAsRole(browser, plan.baseUrl, role, groundedPages, plan.focusTerms ?? []);
    roles.push(result);
    const login = keptLogin(role.loginActions);
    groundedRoles.push({
      role: result.role,
      loginSucceeded: result.loginSucceeded,
      loginError: result.loginError,
      loginUsesLiterals: login.usesLiterals,
      loginActions: login.actions,
      pages: groundedPages,
    });
  }
  const visibilityMatrix = computeVisibilityMatrix(roles);
  const output: CrawlOutput = { roles, visibilityMatrix };

  console.log(
    `AIQA_CRAWL_SUMMARY ${JSON.stringify({
      roles: roles.map((r) => ({
        role: r.role,
        loginSucceeded: r.loginSucceeded,
        pagesReached: r.pages.length,
      })),
      urlsWithRoleOnlyElements: visibilityMatrix.filter(
        (e) => Object.keys(e.elementsOnlyVisibleTo).length > 0
      ).length,
    })}`
  );

  const filePath = path.join(__dirname, "..", "tests", "generated", resultFileName);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, JSON.stringify(output, null, 2));

  // Beside the result, what test cases can be compiled on (`services/orchestrator/src/grounding.rs` reads it).
  const grounding: CrawlGrounding = { roles: groundedRoles };
  fs.writeFileSync(
    path.join(path.dirname(filePath), groundingFileName(resultFileName)),
    JSON.stringify(grounding, null, 2)
  );
}

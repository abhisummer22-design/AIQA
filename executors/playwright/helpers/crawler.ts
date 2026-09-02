import { Browser } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";

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
const NAV_TIMEOUT_MS = 8000;

function sameOrigin(a: string, b: string): boolean {
  try {
    return new URL(a).origin === new URL(b).origin;
  } catch {
    return false;
  }
}

async function runLoginActions(
  page: import("@playwright/test").Page,
  actions: CrawlAction[]
): Promise<void> {
  for (const action of actions) {
    switch (action.type) {
      case "NAVIGATE":
        await page.goto(action.url!, { timeout: NAV_TIMEOUT_MS });
        break;
      case "FILL":
        await page.locator(action.selector!).fill(action.value!, { timeout: NAV_TIMEOUT_MS });
        break;
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

/** One role's real crawl: real login (if configured), then a real,
 * bounded, same-origin breadth-first crawl from `baseUrl`, recording every
 * page's real title and real interactive elements. Never invents a page
 * this role's own session didn't actually reach. */
async function crawlAsRole(
  browser: Browser,
  baseUrl: string,
  role: CrawlRoleConfig
): Promise<RoleCrawlResult> {
  const context = await browser.newContext();
  const page = await context.newPage();

  let loginSucceeded = true;
  let loginError: string | undefined;
  try {
    await runLoginActions(page, role.loginActions);
  } catch (e) {
    loginSucceeded = false;
    loginError = String(e);
  }

  const visited = new Set<string>();
  const queue: string[] = [baseUrl];
  const pages: CrawledPage[] = [];

  while (queue.length > 0 && pages.length < MAX_PAGES_PER_ROLE) {
    const url = queue.shift()!;
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

    for (const el of elements) {
      if (el.kind === "link" && el.href && sameOrigin(el.href, baseUrl) && !visited.has(el.href)) {
        queue.push(el.href);
      }
    }
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
  for (const role of plan.roles) {
    roles.push(await crawlAsRole(browser, plan.baseUrl, role));
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
}

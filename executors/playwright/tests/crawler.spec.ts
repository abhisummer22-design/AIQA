import { test, expect } from "@playwright/test";
import { ChildProcess, spawn } from "child_process";
import * as fs from "fs";
import * as path from "path";
import {
  CrawlPlan,
  MAX_FOCUS_EXTRA_PAGES,
  enqueueLinks,
  extractGroundedElements,
  extractHeadings,
  focusScore,
  groundingFileName,
  runCrawl,
  takeNext,
} from "../helpers/crawler";
import { resolveValue } from "../helpers/secrets";

// Real, self-contained verification (same "own fixture, real browser, real
// pass/fail" pattern tests/sample.spec.ts and tests/healing.spec.ts already
// use): starts the real fixture app (fixtures/role_gated_app.js) as a real
// child process, then runs `runCrawl` — the exact function a materialized
// crawl spec calls — against it with real role credentials, and asserts on
// the real, resulting per-role page reach and visibility matrix. Proves the
// mechanism, not a scripted stand-in for it.

const FIXTURE_PORT = 4174;
const BASE_URL = `http://127.0.0.1:${FIXTURE_PORT}`;
const RESULT_DIR = path.join(__dirname, "generated");

let fixtureProcess: ChildProcess;

test.beforeAll(async () => {
  fixtureProcess = spawn(
    "node",
    [path.join(__dirname, "..", "fixtures", "role_gated_app.js")],
    { env: { ...process.env, FIXTURE_PORT: String(FIXTURE_PORT) }, stdio: "pipe" }
  );
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(
      () => reject(new Error("fixture server did not start in time")),
      10000
    );
    fixtureProcess.stdout?.on("data", (data) => {
      if (data.toString().includes("listening")) {
        clearTimeout(timeout);
        resolve();
      }
    });
    fixtureProcess.on("error", reject);
  });
});

test.afterAll(() => {
  fixtureProcess.kill();
});

function loginActions(username: string, password: string) {
  return [
    { type: "NAVIGATE" as const, url: `${BASE_URL}/login` },
    { type: "FILL" as const, selector: "#username", value: username },
    { type: "FILL" as const, selector: "#password", value: password },
    { type: "CLICK" as const, selector: "#submit-login" },
  ];
}

function pathsOf(pages: { url: string }[]): string[] {
  return pages.map((p) => new URL(p.url).pathname).sort();
}

test("crawling as admin, user, and anonymous produces a real, distinct per-role visibility matrix", async ({
  browser,
}) => {
  const plan: CrawlPlan = {
    baseUrl: BASE_URL,
    roles: [
      { name: "admin", loginActions: loginActions("admin", "admin-pass") },
      { name: "user", loginActions: loginActions("alice", "alice-pass") },
      { name: "anonymous", loginActions: [] },
    ],
  };

  const resultFile = "crawl-result-verification.json";
  await runCrawl(browser, plan, resultFile);
  const output = JSON.parse(fs.readFileSync(path.join(RESULT_DIR, resultFile), "utf-8"));

  const admin = output.roles.find((r: any) => r.role === "admin");
  const user = output.roles.find((r: any) => r.role === "user");
  const anon = output.roles.find((r: any) => r.role === "anonymous");

  expect(admin.loginSucceeded).toBe(true);
  expect(user.loginSucceeded).toBe(true);

  // The real, structural difference a per-role crawl exists to find: admin
  // reaches pages user's own real session never can.
  const adminPaths = pathsOf(admin.pages);
  const userPaths = pathsOf(user.pages);
  const anonPaths = pathsOf(anon.pages);
  expect(adminPaths).toContain("/settings");
  expect(adminPaths).toContain("/users");
  expect(userPaths).not.toContain("/settings");
  expect(userPaths).not.toContain("/users");
  expect(userPaths).toContain("/dashboard");
  expect(anonPaths).toEqual(["/login"]);

  // The real visibility matrix, not just per-role page lists: /dashboard is
  // reachable by both admin and user, but admin's own session is the only
  // one that ever actually saw the "Settings"/"Manage Users" nav links
  // there — the concrete signal an RBAC test generator would consume.
  const dashboardDiff = output.visibilityMatrix.find(
    (e: any) => new URL(e.url).pathname === "/dashboard"
  );
  expect(dashboardDiff, `visibilityMatrix=${JSON.stringify(output.visibilityMatrix)}`).toBeTruthy();
  expect(dashboardDiff.rolesWithAccess.sort()).toEqual(["admin", "user"]);
  expect(dashboardDiff.elementsOnlyVisibleTo.admin).toContain("Settings");
  expect(dashboardDiff.elementsOnlyVisibleTo.admin).toContain("Manage Users");
  expect(dashboardDiff.elementsOnlyVisibleTo.user ?? []).not.toContain("Dashboard");
});

test("wrong credentials never reach a real role-gated page, no fabricated success", async ({
  browser,
}) => {
  const plan: CrawlPlan = {
    baseUrl: BASE_URL,
    roles: [{ name: "bad-credentials", loginActions: loginActions("admin", "totally-wrong") }],
  };
  const resultFile = "crawl-result-bad-credentials.json";
  await runCrawl(browser, plan, resultFile);
  const output = JSON.parse(
    fs.readFileSync(path.join(RESULT_DIR, resultFile), "utf-8")
  );
  const result = output.roles[0];
  // The click itself doesn't throw (the button is real and clickable) — the
  // real signal that the credentials were rejected is that this session
  // never reaches ANY role-gated page, not a contrived boolean flag.
  const paths = pathsOf(result.pages);
  expect(paths).not.toContain("/dashboard");
  expect(paths).not.toContain("/settings");
  expect(paths).not.toContain("/users");
});

test("a login selector that does not exist is a real, honest loginSucceeded: false", async ({
  browser,
}) => {
  const plan: CrawlPlan = {
    baseUrl: BASE_URL,
    roles: [
      {
        name: "broken-login-flow",
        loginActions: [
          { type: "NAVIGATE", url: `${BASE_URL}/login` },
          { type: "FILL", selector: "#this-selector-does-not-exist", value: "x" },
        ],
      },
    ],
  };
  const resultFile = "crawl-result-broken-login.json";
  await runCrawl(browser, plan, resultFile);
  const output = JSON.parse(
    fs.readFileSync(path.join(RESULT_DIR, resultFile), "utf-8")
  );
  expect(output.roles[0].loginSucceeded).toBe(false);
  expect(output.roles[0].loginError).toBeTruthy();
});

// Run-time secrets: a login written as `env:NAME` references reads its values from the environment when the crawl
// runs (helpers/secrets.ts). The plan a spec embeds holds only the names.

const SECRET_NAMES = ["AIQA_LOGIN_CRAWLTEST_USER", "AIQA_LOGIN_CRAWLTEST_PW"];

test.afterEach(() => {
  for (const name of SECRET_NAMES) delete process.env[name];
});

function referencedLogin() {
  return [
    { type: "NAVIGATE" as const, url: `${BASE_URL}/login` },
    { type: "FILL" as const, selector: "#username", value: "env:AIQA_LOGIN_CRAWLTEST_USER" },
    { type: "FILL" as const, selector: "#password", value: "env:AIQA_LOGIN_CRAWLTEST_PW" },
    { type: "CLICK" as const, selector: "#submit-login" },
  ];
}

test("a login written as env:NAME references logs in with the environment's values", async ({ browser }) => {
  process.env.AIQA_LOGIN_CRAWLTEST_USER = "admin";
  process.env.AIQA_LOGIN_CRAWLTEST_PW = "admin-pass";
  const plan: CrawlPlan = { baseUrl: BASE_URL, roles: [{ name: "admin", loginActions: referencedLogin() }] };
  const resultFile = "crawl-result-env-refs.json";
  await runCrawl(browser, plan, resultFile);
  const raw = fs.readFileSync(path.join(RESULT_DIR, resultFile), "utf-8");
  const admin = JSON.parse(raw).roles[0];

  expect(admin.loginSucceeded).toBe(true);
  expect(pathsOf(admin.pages)).toContain("/settings");
  // The result the crawl writes carries no credential.
  expect(raw).not.toContain("admin-pass");
});

test("a login whose secret is not set fails honestly, naming the variable and never a value", async ({ browser }) => {
  process.env.AIQA_LOGIN_CRAWLTEST_USER = "admin";
  delete process.env.AIQA_LOGIN_CRAWLTEST_PW;
  const plan: CrawlPlan = { baseUrl: BASE_URL, roles: [{ name: "admin", loginActions: referencedLogin() }] };
  const resultFile = "crawl-result-env-missing.json";
  await runCrawl(browser, plan, resultFile);
  const admin = JSON.parse(fs.readFileSync(path.join(RESULT_DIR, resultFile), "utf-8")).roles[0];

  expect(admin.loginSucceeded).toBe(false);
  expect(admin.loginError).toContain("AIQA_LOGIN_CRAWLTEST_PW is not set");
  expect(pathsOf(admin.pages)).not.toContain("/dashboard");
});

test("a wrong secret is a rejected login, and the value is in neither the result nor the error", async ({ browser }) => {
  process.env.AIQA_LOGIN_CRAWLTEST_USER = "admin";
  process.env.AIQA_LOGIN_CRAWLTEST_PW = "totally-wrong-password";
  const plan: CrawlPlan = { baseUrl: BASE_URL, roles: [{ name: "admin", loginActions: referencedLogin() }] };
  const resultFile = "crawl-result-env-wrong.json";
  await runCrawl(browser, plan, resultFile);
  const raw = fs.readFileSync(path.join(RESULT_DIR, resultFile), "utf-8");
  const admin = JSON.parse(raw).roles[0];

  expect(pathsOf(admin.pages)).not.toContain("/dashboard");
  expect(raw).not.toContain("totally-wrong-password");
});

// Grounding: beside its result a crawl records what a test can act on — every element with a locator the crawl
// checked matches exactly one element on its page — and each role's login without its credentials.

const GROUNDING_FILE = "crawl-grounding-grounding.json";

test("a crawl records grounding: fields and links with locators, and logins without credentials", async ({ browser }) => {
  process.env.AIQA_LOGIN_CRAWLTEST_USER = "admin";
  process.env.AIQA_LOGIN_CRAWLTEST_PW = "admin-pass";
  const plan: CrawlPlan = {
    baseUrl: BASE_URL,
    roles: [
      { name: "admin", loginActions: referencedLogin() },
      { name: "literal", loginActions: loginActions("alice", "alice-pass") },
      { name: "anonymous", loginActions: [] },
    ],
  };
  await runCrawl(browser, plan, "crawl-result-grounding.json");
  const raw = fs.readFileSync(path.join(RESULT_DIR, GROUNDING_FILE), "utf-8");
  const grounding = JSON.parse(raw);
  const role = (name: string) => grounding.roles.find((r: any) => r.role === name);
  const admin = role("admin");
  const literal = role("literal");
  const anonymous = role("anonymous");

  // No credential is in the file, whichever way the login was written.
  expect(raw).not.toContain("admin-pass");
  expect(raw).not.toContain("alice-pass");

  // A login written as references is kept as they were written and can be run again by a test.
  expect(admin.loginUsesLiterals).toBe(false);
  expect(admin.loginActions.find((a: any) => a.selector === "#password").value).toBe("env:AIQA_LOGIN_CRAWLTEST_PW");
  expect(admin.loginActions.find((a: any) => a.selector === "#username").value).toBe("env:AIQA_LOGIN_CRAWLTEST_USER");
  // A login written with literals is kept as steps with the values removed, and flagged.
  expect(literal.loginUsesLiterals).toBe(true);
  expect(literal.loginActions.filter((a: any) => a.type === "FILL").every((a: any) => a.value === "")).toBe(true);
  // The anonymous role logs in with nothing to keep.
  expect(anonymous.loginActions).toEqual([]);
  expect(anonymous.loginUsesLiterals).toBe(false);

  // The login page (seen by the anonymous crawl) offers its fields, not only its button.
  const loginPage = anonymous.pages.find((p: any) => new URL(p.url).pathname === "/login");
  const locators = loginPage.elements.map((e: any) => e.locator);
  expect(locators).toEqual(expect.arrayContaining(["#username", "#password", "#submit-login"]));
  const password = loginPage.elements.find((e: any) => e.locator === "#password");
  expect(password.kind).toBe("input");
  expect(password.inputType).toBe("password");
  expect(loginPage.elements.find((e: any) => e.locator === "#submit-login").kind).toBe("button");

  // What a role sees behind its login is offered with locators that name each link by where it goes.
  const dashboard = admin.pages.find((p: any) => new URL(p.url).pathname === "/dashboard");
  expect(dashboard.elements.map((e: any) => e.locator)).toEqual(
    expect.arrayContaining(['a[href="/settings"]', 'a[href="/users"]'])
  );
  const settings = admin.pages.find((p: any) => new URL(p.url).pathname === "/settings");
  expect(settings.elements.map((e: any) => e.locator)).toContain("#danger-zone");
  // ...and none of it is offered to the role that cannot reach it.
  expect(role("anonymous").pages.some((p: any) => new URL(p.url).pathname === "/settings")).toBe(false);
});

test("every locator a crawl records matches exactly one element when a test uses it", async ({ browser }) => {
  process.env.AIQA_LOGIN_CRAWLTEST_USER = "admin";
  process.env.AIQA_LOGIN_CRAWLTEST_PW = "admin-pass";
  const plan: CrawlPlan = { baseUrl: BASE_URL, roles: [{ name: "admin", loginActions: referencedLogin() }] };
  await runCrawl(browser, plan, "crawl-result-grounding-replay.json");
  const admin = JSON.parse(
    fs.readFileSync(path.join(RESULT_DIR, "crawl-grounding-grounding-replay.json"), "utf-8")
  ).roles[0];

  // Replay it as a test would: sign in from the recorded steps (secrets read from the environment), then use each
  // recorded locator on the page it was recorded on.
  const context = await browser.newContext();
  const page = await context.newPage();
  for (const action of admin.loginActions) {
    if (action.type === "NAVIGATE") await page.goto(action.url);
    if (action.type === "FILL") await page.locator(action.selector).fill(resolveValue(action.value));
    if (action.type === "CLICK") await page.locator(action.selector).click();
  }
  let checked = 0;
  for (const recorded of admin.pages) {
    // Skip the page the crawl left by following /logout: that ends the session it was replaying.
    if (new URL(recorded.url).pathname === "/login") continue;
    await page.goto(recorded.url);
    for (const element of recorded.elements) {
      expect(await page.locator(element.locator).count(), `${recorded.url} ${element.locator}`).toBe(1);
      checked += 1;
    }
  }
  await context.close();
  expect(checked).toBeGreaterThan(5);
});

test("grounded elements: a locator is offered only when it is unique, hidden things are not offered, and each kind is named", async ({ page }) => {
  await page.setContent(`
    <button id="save">Save</button>
    <button>Cancel</button><button>Cancel</button>
    <a href="/one">Docs</a><a href="/two">Docs</a>
    <input id="email" type="email" placeholder="Email address">
    <input type="password" name="pw" aria-label="Password">
    <input type="checkbox" id="remember"><label for="remember">Remember me</label>
    <select name="plan" id="plan"><option>A</option></select>
    <textarea data-testid="notes"></textarea>
    <div style="display:none"><button id="hidden-btn">Hidden</button></div>
    <input type="hidden" name="csrf" value="x">
    <button data-test="go">  Go   now </button>
    <button id="dup">Dup one</button><button id="dup">Dup two</button>
    <button>Unique Label</button>
    <input type="submit" value="Send it">
  `);
  const found = await extractGroundedElements(page);
  const byLocator = new Map(found.map((e) => [e.locator, e]));

  // Each is named by the best locator that matches it alone.
  for (const locator of [
    "#save",
    '[data-test="go"]',
    'a[href="/one"]',
    'a[href="/two"]',
    "#email",
    'input[name="pw"]',
    "#remember",
    "#plan",
    '[data-testid="notes"]',
    'role=button[name="Unique Label"]',
  ]) {
    expect(byLocator.has(locator), `${locator} in ${[...byLocator.keys()].join(" | ")}`).toBe(true);
  }
  // Two buttons that cannot be told apart are not offered at all: a plan built on them would click the wrong one.
  expect(found.filter((e) => e.text === "Cancel")).toEqual([]);
  // A duplicated id is not a locator; the accessible name is used when it is unique.
  expect(byLocator.has("#dup")).toBe(false);
  expect(byLocator.has('role=button[name="Dup one"]')).toBe(true);
  // Nothing hidden is offered.
  expect(found.some((e) => e.text === "Hidden" || e.locator === "#hidden-btn")).toBe(false);
  expect(found.some((e) => e.locator.includes("csrf"))).toBe(false);

  // Each kind is named, with what a person would call the element.
  expect(byLocator.get("#email")).toMatchObject({ kind: "input", inputType: "email", text: "Email address" });
  expect(byLocator.get('input[name="pw"]')).toMatchObject({ kind: "input", inputType: "password", text: "Password" });
  expect(byLocator.get("#remember")).toMatchObject({ kind: "input", inputType: "checkbox", text: "Remember me" });
  expect(byLocator.get("#plan")).toMatchObject({ kind: "select" });
  expect(byLocator.get('[data-testid="notes"]')).toMatchObject({ kind: "textarea" });
  expect(byLocator.get('[data-test="go"]')).toMatchObject({ kind: "button", text: "Go now" });
  expect(byLocator.get('a[href="/one"]')).toMatchObject({ kind: "link", text: "Docs" });
  expect(found.find((e) => e.text === "Send it")).toMatchObject({ kind: "button" });

  // And every locator offered really matches one element.
  for (const element of found) {
    expect(await page.locator(element.locator).count(), element.locator).toBe(1);
  }
});

// ---- Requirement-driven crawling (Phase 6): which links a crawl follows first ---------------------------------------------

test("focusScore counts the focus terms a link mentions in its text or in the path of its address", () => {
  expect(focusScore("Manage Users", "http://x.test/admin/users", ["users"])).toBe(1);
  expect(focusScore("", "http://x.test/reports/monthly", ["report", "monthly"])).toBe(2);
  expect(focusScore("REPORTS", "http://x.test/", ["report"])).toBe(1);
  // The query string is not part of what a link is about.
  expect(focusScore("Home", "http://x.test/?q=report", ["report"])).toBe(0);
  expect(focusScore("Home", "http://x.test/", [])).toBe(0);
  expect(focusScore("Home", "http://x.test/", [""])).toBe(0);
  expect(focusScore("x", "not a url/reports", ["report"])).toBe(1);
});

test("enqueueLinks puts links that mention a focus term first, most terms first, and the rest after", () => {
  const queue = ["http://x.test/a"];
  const focus: string[] = [];
  const terms = ["report", "monthly"];
  enqueueLinks(
    queue,
    focus,
    [
      { text: "About", href: "http://x.test/about" },
      { text: "Reports", href: "http://x.test/reports" },
      { text: "Monthly reports", href: "http://x.test/reports/monthly" },
      { text: "Help", href: "http://x.test/help" },
      { text: "Reports again", href: "http://x.test/reports" },
    ],
    terms
  );
  expect(focus).toEqual(["http://x.test/reports/monthly", "http://x.test/reports"]);
  expect(queue).toEqual(["http://x.test/a", "http://x.test/about", "http://x.test/help"]);
  // A link found later that scores the same as one found earlier stays behind it.
  enqueueLinks(queue, focus, [{ text: "More", href: "http://x.test/other-reports" }], terms);
  expect(focus).toEqual(["http://x.test/reports/monthly", "http://x.test/reports", "http://x.test/other-reports"]);
  // With no terms every link is an ordinary one, in the order found.
  const plain: string[] = [];
  const none: string[] = [];
  enqueueLinks(plain, none, [{ text: "Reports", href: "http://x.test/r" }, { text: "Help", href: "http://x.test/h" }], []);
  expect(plain).toEqual(["http://x.test/r", "http://x.test/h"]);
  expect(none).toEqual([]);
});

test("takeNext follows focus links first, within the usual limit plus the extra, and ordinary links only within the usual limit", () => {
  const queue = ["o1", "o2"];
  const focus = ["f1", "f2"];
  expect(takeNext(queue, focus, 0, true)).toBe("f1");
  expect(takeNext(queue, focus, 0, true)).toBe("f2");
  expect(takeNext(queue, focus, 0, true)).toBe("o1");
  // The usual limit is reached: a focused crawl goes on with focus links, an unfocused one stops.
  expect(takeNext(["o"], ["f"], 20, false)).toBeUndefined();
  expect(takeNext(["o"], ["f"], 20, true)).toBe("f");
  // The extra is used up.
  expect(takeNext(["o"], ["f"], 20 + MAX_FOCUS_EXTRA_PAGES, true)).toBeUndefined();
  // Ordinary links past the usual limit are never followed, focused or not.
  expect(takeNext(["o"], [], 20, true)).toBeUndefined();
  expect(takeNext([], [], 0, false)).toBeUndefined();
});

test("a focused crawl visits the pages its focus terms lead to first; without focus terms the order is what it always was", async ({
  browser,
}) => {
  const plan = (focusTerms?: string[]): CrawlPlan => ({
    baseUrl: BASE_URL,
    roles: [{ name: "admin", loginActions: loginActions("admin", "admin-pass") }],
    ...(focusTerms ? { focusTerms } : {}),
  });
  const visited = async (file: string, p: CrawlPlan): Promise<string[]> => {
    await runCrawl(browser, p, file);
    const output = JSON.parse(fs.readFileSync(path.join(RESULT_DIR, file), "utf-8"));
    return output.roles[0].pages.map((pg: { url: string }) => new URL(pg.url).pathname);
  };
  const off = await visited("crawl-result-focus-off.json", plan());
  const on = await visited("crawl-result-focus-on.json", plan(["users"]));
  for (const f of ["focus-off", "focus-on"]) {
    fs.rmSync(path.join(RESULT_DIR, `crawl-result-${f}.json`), { force: true });
    fs.rmSync(path.join(RESULT_DIR, groundingFileName(`crawl-result-${f}.json`)), { force: true });
  }
  // The dashboard links to Profile, Settings, Manage Users in that order.
  expect(off.indexOf("/profile")).toBeLessThan(off.indexOf("/settings"));
  expect(off.indexOf("/settings")).toBeLessThan(off.indexOf("/users"));
  expect(on.indexOf("/users")).toBeGreaterThan(-1);
  expect(on.indexOf("/users")).toBeLessThan(on.indexOf("/profile"));
  expect(on.indexOf("/users")).toBeLessThan(on.indexOf("/settings"));
  // A small application: the same pages either way, only in a different order.
  expect([...on].sort()).toEqual([...off].sort());
});

test("a crawl records each page's headings beside its elements", async ({ browser }) => {
  const plan: CrawlPlan = {
    baseUrl: BASE_URL,
    roles: [{ name: "admin", loginActions: loginActions("admin", "admin-pass") }],
  };
  const file = "crawl-result-headings.json";
  await runCrawl(browser, plan, file);
  const grounding = JSON.parse(fs.readFileSync(path.join(RESULT_DIR, groundingFileName(file)), "utf-8"));
  fs.rmSync(path.join(RESULT_DIR, file), { force: true });
  fs.rmSync(path.join(RESULT_DIR, groundingFileName(file)), { force: true });
  const byPath = (p: string) => grounding.roles[0].pages.find((pg: { url: string }) => new URL(pg.url).pathname === p);
  expect(byPath("/dashboard").headings).toEqual(["Dashboard"]);
  expect(byPath("/settings").headings).toEqual(["Settings"]);
  expect(byPath("/users").headings).toEqual(["Manage Users"]);
});

test("headings are the visible h1-h3 and role=heading texts, squashed, each once, the first twelve, cut at 80 characters", async ({
  page,
}) => {
  const many = Array.from({ length: 20 }, (_, i) => `<h2>Section ${i}</h2>`).join("");
  await page.setContent(
    `<h1>Shown</h1><h2 style="display:none">Hidden</h2><h3>  Shown   twice </h3><h3>Shown twice</h3>` +
      `<div role="heading">Role heading</div><h4>Not a heading here</h4><h3></h3><h3 style="visibility:hidden">Also hidden</h3>` +
      `<h2>${"x".repeat(200)}</h2>`
  );
  const few = await extractHeadings(page);
  expect(few).toEqual(["Shown", "Shown twice", "Role heading", "x".repeat(80)]);
  await page.setContent(many);
  const capped = await extractHeadings(page);
  expect(capped).toHaveLength(12);
  expect(capped[0]).toBe("Section 0");
  expect(capped[11]).toBe("Section 11");
});

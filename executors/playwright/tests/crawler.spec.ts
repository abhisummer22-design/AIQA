import { test, expect } from "@playwright/test";
import { ChildProcess, spawn } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { CrawlPlan, runCrawl } from "../helpers/crawler";

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

import { test, expect } from "@playwright/test";
import { resolveValue, scrub, secret } from "../helpers/secrets";

// The run-time secret helper (helpers/secrets.ts): pure functions, no browser. What matters is what they refuse
// and that no message they raise ever quotes a value.

const NAMES = [
  "AIQA_LOGIN_UNIT_A",
  "AIQA_LOGIN_UNIT_BLANK",
  "AIQA_LOGIN_UNIT_UNSET",
  "AIQA_SECRET_UNIT_B",
  "GROQ_API_KEY_UNIT",
];

test.afterEach(() => {
  for (const name of NAMES) delete process.env[name];
});

test("secret() reads a reserved variable, trimmed", () => {
  process.env.AIQA_LOGIN_UNIT_A = "  s3cret-value  ";
  process.env.AIQA_SECRET_UNIT_B = "other-value";
  expect(secret("AIQA_LOGIN_UNIT_A")).toBe("s3cret-value");
  expect(secret("AIQA_SECRET_UNIT_B")).toBe("other-value");
});

test("secret() names the variable when it is not set or holds only blanks", () => {
  process.env.AIQA_LOGIN_UNIT_BLANK = "   ";
  expect(() => secret("AIQA_LOGIN_UNIT_UNSET")).toThrow("AIQA secret AIQA_LOGIN_UNIT_UNSET is not set");
  expect(() => secret("AIQA_LOGIN_UNIT_BLANK")).toThrow("AIQA secret AIQA_LOGIN_UNIT_BLANK is not set");
});

test("secret() refuses a variable outside the reserved names even when it is set, and never quotes a value", () => {
  process.env.GROQ_API_KEY_UNIT = "leak-me-please";
  let message = "";
  try {
    secret("GROQ_API_KEY_UNIT");
  } catch (e) {
    message = String(e);
  }
  expect(message).toContain("not allowed");
  expect(message).not.toContain("leak-me-please");
  expect(() => secret("PATH")).toThrow("not allowed");
  expect(() => secret("AIQA_LOGIN_" + "X".repeat(60))).toThrow("not allowed");
  expect(() => secret("AIQA_LOGIN_A B")).toThrow("not allowed");
});

test("resolveValue passes an ordinary value through and resolves env:NAME", () => {
  process.env.AIQA_LOGIN_UNIT_A = "resolved-value";
  expect(resolveValue("admin")).toBe("admin");
  expect(resolveValue("")).toBe("");
  expect(resolveValue("env:AIQA_LOGIN_UNIT_A")).toBe("resolved-value");
  expect(resolveValue("env: AIQA_LOGIN_UNIT_A ")).toBe("resolved-value");
});

test("resolveValue treats any value starting env: as a reference, so a malformed one is an error, never typed text", () => {
  expect(() => resolveValue("env:")).toThrow("not allowed");
  expect(() => resolveValue("env:GROQ_API_KEY")).toThrow("not allowed");
  expect(() => resolveValue("env:AIQA_LOGIN_UNIT_UNSET")).toThrow("is not set");
});

test("scrub replaces a secret in every spelling, longest first, and leaves short values alone", () => {
  const values = ["pa55-w0rd", 'q"uote\\d', "abc"];
  const text = `typed pa55-w0rd; as JSON ${JSON.stringify('q"uote\\d')} and raw q"uote\\d; abc stays`;
  const out = scrub(text, values);
  expect(out).not.toContain("pa55-w0rd");
  expect(out).not.toContain('q"uote\\d');
  expect(out).not.toContain(JSON.stringify('q"uote\\d').slice(1, -1));
  expect(out).toContain("abc stays");
  expect(scrub("nothing here", values)).toBe("nothing here");
  expect(scrub("anything", [])).toBe("anything");
  expect(scrub("value: super-secret-pass", ["secret", "super-secret-pass"])).toBe("value: ***");
});

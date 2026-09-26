/**
 * Run-time secrets for generated specs and crawls.
 *
 * A `FILL` value written `env:NAME` in a plan (`execution_plan.rs::secret_name`) is not text to type but the name of
 * an environment variable of the orchestrator process, which the browser tests inherit. The generated spec holds only
 * the name; the value is read here, when the test runs, so a password is never written into a spec, a plan, a stored
 * run parameter or a log. The orchestrator refuses to start a run whose variables are not set and scrubs their values
 * from the run's captured output (`executor.rs::redact_secrets`); this file is the last check, at the point of use.
 */

/** The reserved prefixes — the same list as `execution_plan.rs::SECRET_NAME_PREFIXES`. Nothing else can be
 * referenced, so a plan cannot ask for `GROQ_API_KEY` to be typed into a page. */
const SECRET_NAME = /^(?:AIQA_LOGIN_|AIQA_SECRET_)[A-Za-z0-9_]*$/;
const MAX_SECRET_NAME_LENGTH = 64;

/** The shortest value `scrub` replaces: a shorter one would also match ordinary words. */
const MIN_SCRUBBED_LENGTH = 4;

/** The value of the environment variable `name`, which must be an `AIQA_LOGIN_*` / `AIQA_SECRET_*` name and must
 * be set. The errors name the variable, never a value. */
export function secret(name: string): string {
  if (!SECRET_NAME.test(name) || name.length > MAX_SECRET_NAME_LENGTH) {
    throw new Error(
      `AIQA secret name ${JSON.stringify(name)} is not allowed: only AIQA_LOGIN_* and AIQA_SECRET_* names can be referenced`
    );
  }
  const value = process.env[name];
  if (value === undefined || value.trim() === "") {
    throw new Error(`AIQA secret ${name} is not set in the orchestrator's environment`);
  }
  return value.trim();
}

/** For a login sequence: a value written `env:NAME` is the secret it names, anything else is the value itself. Any
 * value starting `env:` is a reference (a malformed one throws), as on the Rust side. */
export function resolveValue(value: string): string {
  return value.startsWith("env:") ? secret(value.slice("env:".length).trim()) : value;
}

/** `text` with every value in `values` replaced by `***`, in the form it is written and in the form a JSON string or
 * a source line would spell it. An error's message can quote what was typed. */
export function scrub(text: string, values: string[]): string {
  const forms: string[] = [];
  for (const value of values) {
    if (value.length < MIN_SCRUBBED_LENGTH) continue;
    forms.push(value);
    const quoted = JSON.stringify(value).slice(1, -1);
    if (quoted !== value) forms.push(quoted);
  }
  // Longest first, so a secret that contains another is not left half-scrubbed.
  forms.sort((a, b) => b.length - a.length);
  return forms.reduce((out, form) => out.split(form).join("***"), text);
}

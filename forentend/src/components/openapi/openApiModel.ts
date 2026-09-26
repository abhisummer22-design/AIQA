/**
 * Phase 8B — API tests made from an OpenAPI document: what the form holds and asks the service, and the words for what comes back.
 * The service (`openapi_api.rs`) makes the cases, runs them and adds up the report; nothing here works any of that out again.
 */
import type {
  OpenApiCaseKind,
  OpenApiCounts,
  OpenApiOutcome,
  OpenApiReportCase,
  OpenApiRequestBody,
  OpenApiVerdict,
} from '../../types/contract';
import { isHttpAddress } from '../autopilot/autopilotModel';

/** The kinds of case, in the order the service makes them. */
export const KIND_ORDER: readonly OpenApiCaseKind[] = ['HAPPY', 'NO_AUTH', 'MISSING_REQUIRED', 'WRONG_TYPE', 'NOT_FOUND'];

export const KIND_LABEL: Record<OpenApiCaseKind, { title: string; hint: string }> = {
  HAPPY: { title: 'Valid request', hint: 'a valid request gets a documented success and a body that fits its schema' },
  NO_AUTH: { title: 'Without a credential', hint: 'an operation that needs a credential refuses a request without one (401 or 403)' },
  MISSING_REQUIRED: { title: 'Missing required', hint: 'one required parameter or body property left out: the documented client error' },
  WRONG_TYPE: { title: 'Wrong type', hint: 'a value of the wrong kind (text where a number is due): the documented client error' },
  NOT_FOUND: { title: 'Not found', hint: 'an id that is not there, for an operation that documents a 404' },
};

export const MAX_CASES_LIMIT = 20;
export const DEFAULT_MAX_CASES = 8;

/** The names a credential's variable may start with (`execution_plan::SECRET_NAME_PREFIXES`): a plan may not ask for any other. */
export const VARIABLE_PATTERN = /^(AIQA_LOGIN_|AIQA_SECRET_)[A-Za-z0-9_]*$/;
const HEADER_NAME_PATTERN = /^[A-Za-z0-9-]+$/;

export interface CredentialRow {
  id: number;
  /** The header that carries the credential, like `Authorization`. */
  header: string;
  /** The orchestrator's environment variable that holds its whole value, like `AIQA_SECRET_API_TOKEN`. */
  variable: string;
}

export interface OpenApiFormState {
  source: 'paste' | 'address';
  spec: string;
  specUrl: string;
  baseUrl: string;
  kinds: OpenApiCaseKind[];
  includeUnsafe: boolean;
  credentials: CredentialRow[];
  maxCases: number;
}

export const emptyForm = (): OpenApiFormState => ({
  source: 'paste',
  spec: '',
  specUrl: '',
  baseUrl: '',
  kinds: [...KIND_ORDER],
  includeUnsafe: false,
  credentials: [],
  maxCases: DEFAULT_MAX_CASES,
});

/** What is wrong with the form, in words; empty when the service can be asked. */
export function formProblems(form: OpenApiFormState): string[] {
  const problems: string[] = [];
  if (form.source === 'paste') {
    if (form.spec.trim() === '') problems.push('Paste the OpenAPI document (JSON or YAML).');
  } else if (!isHttpAddress(form.specUrl.trim())) {
    problems.push('The address of the document must start with http:// or https://.');
  }
  if (form.baseUrl.trim() !== '' && !isHttpAddress(form.baseUrl.trim())) {
    problems.push('Where the API is must start with http:// or https://, or be left empty to use the document’s own server.');
  }
  if (form.kinds.length === 0) problems.push('Choose at least one kind of case.');
  if (!Number.isInteger(form.maxCases) || form.maxCases < 1 || form.maxCases > MAX_CASES_LIMIT) {
    problems.push(`Cases for one operation must be from 1 to ${MAX_CASES_LIMIT}.`);
  }
  const seen = new Set<string>();
  for (const row of form.credentials) {
    const header = row.header.trim();
    const variable = row.variable.trim();
    if (header === '' && variable === '') continue;
    if (!HEADER_NAME_PATTERN.test(header)) problems.push(`“${header}” is not a header name (letters, digits and hyphens).`);
    else if (seen.has(header.toLowerCase())) problems.push(`The header ${header} is given twice.`);
    else seen.add(header.toLowerCase());
    if (!VARIABLE_PATTERN.test(variable)) problems.push(`The variable of ${header || 'a header'} must start with AIQA_SECRET_ or AIQA_LOGIN_ (letters, digits and _).`);
  }
  // Two rows with the same fault would say the same sentence twice.
  return [...new Set(problems)];
}

/** The credentials as the service takes them: the header and `env:NAME`, never the credential itself. */
export function authHeaders(credentials: readonly CredentialRow[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const row of credentials) {
    const header = row.header.trim();
    const variable = row.variable.trim();
    if (header !== '' && variable !== '') out[header] = `env:${variable}`;
  }
  return out;
}

/** The body for `plan` and `runs`. `operations` narrows the operations (all when empty or when every one is chosen). */
export function buildBody(form: OpenApiFormState, extra: { operations?: string[]; name?: string } = {}): OpenApiRequestBody {
  const body: OpenApiRequestBody = {
    kinds: form.kinds,
    include_unsafe: form.includeUnsafe,
    max_cases_per_operation: form.maxCases,
  };
  if (form.source === 'paste') body.spec = form.spec;
  else body.spec_url = form.specUrl.trim();
  if (form.baseUrl.trim() !== '') body.base_url = form.baseUrl.trim();
  const headers = authHeaders(form.credentials);
  if (Object.keys(headers).length > 0) body.auth_headers = headers;
  if (extra.operations && extra.operations.length > 0) body.operations = extra.operations;
  if (extra.name && extra.name.trim() !== '') body.name = extra.name.trim();
  return body;
}

// ------------------------------------------------------------------------------------------------------------ the words

export const OUTCOME_LABEL: Record<OpenApiOutcome, string> = {
  PASSED: 'Passed',
  FAILED: 'Failed',
  ERROR: 'Could not run',
  PENDING: 'Waiting to run',
  NOT_RUN: 'Not run',
};

export const VERDICT_LABEL: Record<OpenApiVerdict, string> = {
  ...OUTCOME_LABEL,
  SKIPPED: 'No cases',
  NOT_SELECTED: 'Not selected',
};

export type Tone = 'good' | 'bad' | 'warn' | 'plain';

export function outcomeTone(outcome: OpenApiVerdict): Tone {
  switch (outcome) {
    case 'PASSED':
      return 'good';
    case 'FAILED':
      return 'bad';
    case 'ERROR':
    case 'PENDING':
      return 'warn';
    default:
      return 'plain';
  }
}

/** `200`, `200 or 201`, `404, 400 or 422`: the statuses that pass. */
export function statusesText(statuses: readonly number[]): string {
  if (statuses.length === 0) return 'no status';
  if (statuses.length === 1) return String(statuses[0]);
  return `${statuses.slice(0, -1).join(', ')} or ${statuses[statuses.length - 1]}`;
}

/** What a case expects, in a line: the status, and the schema when the document gives one. */
export function expectText(expect: OpenApiReportCase['expect']): string {
  return `${statusesText(expect.statuses)}${expect.schema ? ' · body fits its schema' : ''}`;
}

/** A request's address without its origin, so a table row stays short: `/pets?limit=1`. */
export function pathOf(url: string): string {
  try {
    const parsed = new URL(url);
    return `${parsed.pathname}${parsed.search}`;
  } catch {
    return url;
  }
}

/** `88%` for 0.875 (rounded), `—` when nothing was run. */
export const passRateText = (rate: number | null): string => (rate === null ? '—' : `${Math.round(rate * 100)}%`);

/** `3 passed · 1 failed · 4 not run`: only what there is. */
export function countsLine(counts: OpenApiCounts | null): string {
  if (!counts) return 'no cases';
  const parts: string[] = [];
  if (counts.passed > 0) parts.push(`${counts.passed} passed`);
  if (counts.failed > 0) parts.push(`${counts.failed} failed`);
  if (counts.error > 0) parts.push(`${counts.error} could not run`);
  if (counts.pending > 0) parts.push(`${counts.pending} waiting`);
  if (counts.not_run > 0) parts.push(`${counts.not_run} not run`);
  return parts.join(' · ') || 'no cases';
}

/** How long to wait before reading a report again: a run that is going is read often, one that has ended never again. */
export const REPORT_POLL_MS = 2_000;

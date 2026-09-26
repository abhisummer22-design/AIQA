/**
 * Phase 8B — what the OpenAPI tests form holds and asks, and the words for what comes back.
 */
import { describe, expect, it } from 'vitest';
import {
  DEFAULT_MAX_CASES,
  KIND_LABEL,
  KIND_ORDER,
  MAX_CASES_LIMIT,
  OUTCOME_LABEL,
  REPORT_POLL_MS,
  VARIABLE_PATTERN,
  VERDICT_LABEL,
  authHeaders,
  buildBody,
  countsLine,
  emptyForm,
  expectText,
  formProblems,
  outcomeTone,
  passRateText,
  pathOf,
  statusesText,
  type OpenApiFormState,
} from './openApiModel';

const valid = (over: Partial<OpenApiFormState> = {}): OpenApiFormState => ({ ...emptyForm(), spec: 'openapi: 3.0.0', ...over });

describe('the form as it starts', () => {
  it('has the document to be pasted, every kind of case, nothing that changes data and no credential', () => {
    expect(emptyForm()).toEqual({
      source: 'paste',
      spec: '',
      specUrl: '',
      baseUrl: '',
      kinds: ['HAPPY', 'NO_AUTH', 'MISSING_REQUIRED', 'WRONG_TYPE', 'NOT_FOUND'],
      includeUnsafe: false,
      credentials: [],
      maxCases: 8,
    });
    expect(KIND_ORDER).toEqual(emptyForm().kinds);
    expect(DEFAULT_MAX_CASES).toBe(8);
    expect(MAX_CASES_LIMIT).toBe(20);
  });

  it('gives each kind a name and a line saying what it is', () => {
    for (const kind of KIND_ORDER) {
      expect(KIND_LABEL[kind].title.length).toBeGreaterThan(3);
      expect(KIND_LABEL[kind].hint.length).toBeGreaterThan(10);
    }
    expect(KIND_LABEL.NO_AUTH.title).toBe('Without a credential');
    expect(KIND_LABEL.HAPPY.title).toBe('Valid request');
  });

  it('gives each one of its own list of copies each time, so one form never changes another', () => {
    const a = emptyForm();
    a.kinds.pop();
    expect(emptyForm().kinds).toHaveLength(5);
  });
});

describe('formProblems', () => {
  it('has none for a form the service can be asked', () => {
    expect(formProblems(valid())).toEqual([]);
  });

  it('asks for the document, pasted or by its address', () => {
    expect(formProblems(emptyForm())).toEqual(['Paste the OpenAPI document (JSON or YAML).']);
    expect(formProblems(valid({ spec: '   \n ' }))).toEqual(['Paste the OpenAPI document (JSON or YAML).']);
    expect(formProblems(valid({ source: 'address', specUrl: '' }))).toEqual(['The address of the document must start with http:// or https://.']);
    expect(formProblems(valid({ source: 'address', specUrl: 'ftp://x.test/a.json' }))).toHaveLength(1);
    expect(formProblems(valid({ source: 'address', specUrl: ' https://x.test/openapi.json ' }))).toEqual([]);
    // A pasted document is not asked for when the address is what counts, and the other way round.
    expect(formProblems(valid({ source: 'address', spec: '', specUrl: 'https://x.test/a.json' }))).toEqual([]);
    expect(formProblems(valid({ source: 'paste', specUrl: 'not an address' }))).toEqual([]);
  });

  it('accepts where the API is when it is an http or https address, or nothing', () => {
    expect(formProblems(valid({ baseUrl: '' }))).toEqual([]);
    expect(formProblems(valid({ baseUrl: '   ' }))).toEqual([]);
    expect(formProblems(valid({ baseUrl: 'https://api.example.com/v1' }))).toEqual([]);
    expect(formProblems(valid({ baseUrl: 'http://localhost:8080/fixture' }))).toEqual([]);
    expect(formProblems(valid({ baseUrl: 'api.example.com' }))).toHaveLength(1);
    expect(formProblems(valid({ baseUrl: 'api.example.com' }))[0]).toContain('Where the API is must start with http:// or https://');
  });

  it('needs a kind of case and a number of cases within the limit', () => {
    expect(formProblems(valid({ kinds: [] }))).toEqual(['Choose at least one kind of case.']);
    for (const bad of [0, -1, 21, 2.5, Number.NaN]) {
      expect(formProblems(valid({ maxCases: bad })), String(bad)).toEqual(['Cases for one operation must be from 1 to 20.']);
    }
    for (const fine of [1, 8, 20]) expect(formProblems(valid({ maxCases: fine })), String(fine)).toEqual([]);
  });

  it('checks each header and the variable that holds its value', () => {
    const rows = (...pairs: Array<[string, string]>) => pairs.map(([header, variable], i) => ({ id: i + 1, header, variable }));
    expect(formProblems(valid({ credentials: rows(['Authorization', 'AIQA_SECRET_API_TOKEN']) }))).toEqual([]);
    expect(formProblems(valid({ credentials: rows(['X-Api-Key', 'AIQA_LOGIN_KEY_2']) }))).toEqual([]);
    // A row left empty is not a problem, it is nothing.
    expect(formProblems(valid({ credentials: rows(['', ''], ['  ', ' ']) }))).toEqual([]);
    expect(formProblems(valid({ credentials: rows(['Bad Header', 'AIQA_SECRET_X']) }))).toEqual(['“Bad Header” is not a header name (letters, digits and hyphens).']);
    expect(formProblems(valid({ credentials: rows(['', 'AIQA_SECRET_X']) }))).toEqual(['“” is not a header name (letters, digits and hyphens).']);
    expect(formProblems(valid({ credentials: rows(['Authorization', 'GROQ_API_KEY']) }))).toEqual([
      'The variable of Authorization must start with AIQA_SECRET_ or AIQA_LOGIN_ (letters, digits and _).',
    ]);
    expect(formProblems(valid({ credentials: rows(['Authorization', '']) }))).toHaveLength(1);
    expect(formProblems(valid({ credentials: rows(['Authorization', 'AIQA_SECRET_A-B']) }))).toHaveLength(1);
    expect(formProblems(valid({ credentials: rows(['Authorization', 'AIQA_SECRET_A'], ['authorization', 'AIQA_SECRET_B']) }))).toEqual(['The header authorization is given twice.']);
  });

  it('says the same thing once when two rows have the same fault, so a list of them has no repeats', () => {
    const rows = (...pairs: Array<[string, string]>) => pairs.map(([header, variable], i) => ({ id: i + 1, header, variable }));
    const twice = formProblems(valid({ credentials: rows(['Authorization', ''], ['Authorization', '']) }));
    // In the order the rows are read: the first row's variable, then the second row's header.
    expect(twice).toEqual([
      'The variable of Authorization must start with AIQA_SECRET_ or AIQA_LOGIN_ (letters, digits and _).',
      'The header Authorization is given twice.',
    ]);
    // A different header is a different sentence, so it is still said.
    expect(formProblems(valid({ credentials: rows(['Authorization', ''], ['X-Api-Key', '']) }))).toHaveLength(2);
  });

  it('says every problem at once', () => {
    const problems = formProblems({ ...emptyForm(), spec: '', kinds: [], baseUrl: 'nope', maxCases: 0 });
    expect(problems).toHaveLength(4);
  });

  it('knows which variables a plan may name', () => {
    expect(VARIABLE_PATTERN.test('AIQA_SECRET_API_TOKEN')).toBe(true);
    expect(VARIABLE_PATTERN.test('AIQA_LOGIN_ADMIN_PASSWORD')).toBe(true);
    expect(VARIABLE_PATTERN.test('GROQ_API_KEY')).toBe(false);
    expect(VARIABLE_PATTERN.test('aiqa_secret_x')).toBe(false);
    expect(VARIABLE_PATTERN.test('XAIQA_SECRET_X')).toBe(false);
    expect(VARIABLE_PATTERN.test('AIQA_SECRET_X Y')).toBe(false);
  });
});

describe('authHeaders', () => {
  it('is each header with its variable as a reference, and never a credential', () => {
    expect(
      authHeaders([
        { id: 1, header: ' Authorization ', variable: ' AIQA_SECRET_API_TOKEN ' },
        { id: 2, header: 'X-Api-Key', variable: 'AIQA_LOGIN_KEY' },
      ]),
    ).toEqual({ Authorization: 'env:AIQA_SECRET_API_TOKEN', 'X-Api-Key': 'env:AIQA_LOGIN_KEY' });
  });

  it('leaves out a row that is not complete', () => {
    expect(authHeaders([{ id: 1, header: 'Authorization', variable: '' }, { id: 2, header: '', variable: 'AIQA_SECRET_X' }])).toEqual({});
    expect(authHeaders([])).toEqual({});
  });
});

describe('buildBody', () => {
  it('sends a pasted document as `spec` with the kinds, the limit and whether unsafe methods are allowed', () => {
    expect(buildBody(valid())).toEqual({
      spec: 'openapi: 3.0.0',
      kinds: KIND_ORDER,
      include_unsafe: false,
      max_cases_per_operation: 8,
    });
  });

  it('sends an address as `spec_url`, trimmed, and not the pasted text', () => {
    const body = buildBody(valid({ source: 'address', specUrl: '  https://x.test/openapi.json ', spec: 'ignored' }));
    expect(body.spec_url).toBe('https://x.test/openapi.json');
    expect(body.spec).toBeUndefined();
  });

  it('sends where the API is only when it was given, trimmed', () => {
    expect(buildBody(valid()).base_url).toBeUndefined();
    expect(buildBody(valid({ baseUrl: '   ' })).base_url).toBeUndefined();
    expect(buildBody(valid({ baseUrl: ' https://api.example.com/v1 ' })).base_url).toBe('https://api.example.com/v1');
  });

  it('sends the credentials only when there are some, as references', () => {
    expect(buildBody(valid()).auth_headers).toBeUndefined();
    expect(buildBody(valid({ credentials: [{ id: 1, header: 'Authorization', variable: 'AIQA_SECRET_T' }] })).auth_headers).toEqual({ Authorization: 'env:AIQA_SECRET_T' });
  });

  it('sends the unsafe choice and the limit as they are', () => {
    const body = buildBody(valid({ includeUnsafe: true, maxCases: 3, kinds: ['HAPPY'] }));
    expect(body.include_unsafe).toBe(true);
    expect(body.max_cases_per_operation).toBe(3);
    expect(body.kinds).toEqual(['HAPPY']);
  });

  it('narrows the operations and names the run only when asked to', () => {
    expect(buildBody(valid()).operations).toBeUndefined();
    expect(buildBody(valid(), { operations: [] }).operations).toBeUndefined();
    expect(buildBody(valid(), { operations: ['GET /pets'] }).operations).toEqual(['GET /pets']);
    expect(buildBody(valid()).name).toBeUndefined();
    expect(buildBody(valid(), { name: '  ' }).name).toBeUndefined();
    expect(buildBody(valid(), { name: ' Nightly ' }).name).toBe('Nightly');
  });
});

describe('the words for what comes back', () => {
  it('names every outcome and every verdict', () => {
    expect(OUTCOME_LABEL).toEqual({ PASSED: 'Passed', FAILED: 'Failed', ERROR: 'Could not run', PENDING: 'Waiting to run', NOT_RUN: 'Not run' });
    expect(VERDICT_LABEL.SKIPPED).toBe('No cases');
    expect(VERDICT_LABEL.NOT_SELECTED).toBe('Not selected');
    expect(VERDICT_LABEL.PASSED).toBe('Passed');
  });

  it('colours a pass green, a failure red, what could not be run or is waiting amber and the rest plain', () => {
    expect(outcomeTone('PASSED')).toBe('good');
    expect(outcomeTone('FAILED')).toBe('bad');
    expect(outcomeTone('ERROR')).toBe('warn');
    expect(outcomeTone('PENDING')).toBe('warn');
    expect(outcomeTone('NOT_RUN')).toBe('plain');
    expect(outcomeTone('SKIPPED')).toBe('plain');
    expect(outcomeTone('NOT_SELECTED')).toBe('plain');
  });

  it('writes the statuses that pass', () => {
    expect(statusesText([])).toBe('no status');
    expect(statusesText([200])).toBe('200');
    expect(statusesText([200, 201])).toBe('200 or 201');
    expect(statusesText([400, 404, 422])).toBe('400, 404 or 422');
  });

  it('writes what a case expects, with the schema when the document gives one', () => {
    expect(expectText({ statuses: [200], schema: false, why: '' })).toBe('200');
    expect(expectText({ statuses: [200, 201], schema: true, why: '' })).toBe('200 or 201 · body fits its schema');
  });

  it('shortens an address to its path and query, and leaves alone what is not an address', () => {
    expect(pathOf('http://127.0.0.1:8080/fixture/pets?limit=1')).toBe('/fixture/pets?limit=1');
    expect(pathOf('https://api.example.com/')).toBe('/');
    expect(pathOf('/already/short')).toBe('/already/short');
    expect(pathOf('')).toBe('');
  });

  it('writes a pass rate as a whole percentage, and a dash when nothing was run', () => {
    expect(passRateText(null)).toBe('—');
    expect(passRateText(0.875)).toBe('88%');
    expect(passRateText(1)).toBe('100%');
    expect(passRateText(0)).toBe('0%');
    expect(passRateText(0.333)).toBe('33%');
  });

  it('sums up an operation’s counts in a line, naming only what there is', () => {
    expect(countsLine(null)).toBe('no cases');
    expect(countsLine({ passed: 0, failed: 0, error: 0, pending: 0, not_run: 0 })).toBe('no cases');
    expect(countsLine({ passed: 2, failed: 1, error: 0, pending: 0, not_run: 0 })).toBe('2 passed · 1 failed');
    expect(countsLine({ passed: 1, failed: 2, error: 3, pending: 4, not_run: 5 })).toBe('1 passed · 2 failed · 3 could not run · 4 waiting · 5 not run');
  });

  it('reads a report that is going every two seconds', () => {
    expect(REPORT_POLL_MS).toBe(2000);
  });
});

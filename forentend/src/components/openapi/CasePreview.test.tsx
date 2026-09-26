/**
 * Phase 8B — the cases the service would make from the document, shown before anything is run.
 */
import React from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { CasePreview, titleWithin } from './CasePreview';
import { OPERATIONS, planFor } from './openApiTestData';
import type { OpenApiPlan } from '../../types/contract';

afterEach(cleanup);

const ALL_WITH_CASES = new Set(OPERATIONS.map((o) => o.id));
const el = (id: string) => document.getElementById(id) as HTMLElement;
const text = (id: string) => el(id)?.textContent ?? '';

function show(plan: OpenApiPlan = planFor(), over: Partial<React.ComponentProps<typeof CasePreview>> = {}) {
  const calls = { toggle: vi.fn(), all: vi.fn(), run: vi.fn() };
  render(
    <CasePreview
      plan={plan}
      selected={ALL_WITH_CASES}
      onToggle={calls.toggle}
      onSelectAll={calls.all}
      onRun={calls.run}
      running={false}
      unsafeAllowed={false}
      {...over}
    />,
  );
  return calls;
}

describe('titleWithin', () => {
  it('drops the operation a case’s title starts with, since it is shown under it', () => {
    expect(titleWithin({ title: 'GET /pets: without the required query parameter limit', operation: 'GET /pets' })).toBe('without the required query parameter limit');
  });

  it('leaves a title that does not start with it', () => {
    expect(titleWithin({ title: 'something else', operation: 'GET /pets' })).toBe('something else');
    expect(titleWithin({ title: 'GET /pets', operation: 'GET /pets' })).toBe('GET /pets');
    expect(titleWithin({ title: 'GET /pets/{id}: x', operation: 'GET /pets' })).toBe('GET /pets/{id}: x');
  });
});

describe('the tiles', () => {
  it('count the operations, the cases and what would run, and say what it would run against', () => {
    show();
    expect(text('openapi-tile-operations')).toContain('4');
    expect(text('openapi-tile-operations')).toContain('Fixture Pets 1.0');
    expect(text('openapi-tile-cases')).toContain('12');
    expect(text('openapi-tile-run')).toContain('8');
    expect(text('openapi-tile-run')).toContain('against http://127.0.0.1:8080/fixture');
  });

  it('count only the operations that are chosen, and only the cases that are not held back', () => {
    show(planFor(), { selected: new Set(['GET /pets']) });
    expect(text('openapi-tile-run')).toMatch(/^To run3/);
    cleanup();
    show(planFor(), { selected: new Set(['POST /pets']) });
    expect(text('openapi-tile-run')).toMatch(/^To run0/);
    cleanup();
    show(planFor(), { selected: new Set() });
    expect(text('openapi-tile-run')).toMatch(/^To run0/);
  });

  it('are amber for the cases that change data, and say what to do about them until that is allowed', () => {
    show();
    expect(el('openapi-tile-not-run').className).toContain('bg-amber-950/30');
    expect(text('openapi-tile-not-run')).toContain('4');
    expect(text('openapi-tile-not-run')).toContain('they change data: allow that above to run them');
    cleanup();
    show(planFor(), { unsafeAllowed: true });
    expect(text('openapi-tile-not-run')).not.toContain('allow that above');
    cleanup();
    show(planFor({ counts: { cases: 8, runnable: 8, not_run: 0 } }));
    expect(el('openapi-tile-not-run').className).not.toContain('amber');
    expect(text('openapi-tile-not-run')).not.toContain('allow that above');
  });
});

describe('the operations', () => {
  it('are each a block with its name, what it does and how many cases it has', () => {
    show();
    expect(text('openapi-op-GET /pets')).toContain('GET /pets');
    expect(text('openapi-op-GET /pets')).toContain('List the pets');
    expect(text('openapi-op-GET /pets')).toContain('3 cases');
    expect(text('openapi-op-POST /pets')).toContain('4 cases');
    expect(text('openapi-op-GET /pets/{petId}')).toContain('3 cases');
    expect(text('openapi-op-GET /secure')).toContain('2 cases');
  });

  it('say when an operation needs a credential or is deprecated, and only then', () => {
    const operations = OPERATIONS.map((o) => (o.id === 'GET /pets' ? { ...o, deprecated: true } : o));
    show(planFor({ operations }));
    expect(text('openapi-op-GET /secure')).toContain('credential');
    expect(text('openapi-op-GET /pets')).not.toContain('credential');
    expect(text('openapi-op-GET /pets')).toContain('deprecated');
    expect(text('openapi-op-GET /secure')).not.toContain('deprecated');
  });

  it('are chosen by a box each, which asks for that operation to be toggled', () => {
    const calls = show(planFor(), { selected: new Set(['GET /pets']) });
    const box = (id: string) => el(`openapi-op-${id}`).querySelector('input[type=checkbox]') as HTMLInputElement;
    expect(box('GET /pets').checked).toBe(true);
    expect(box('POST /pets').checked).toBe(false);
    fireEvent.click(box('POST /pets'));
    expect(calls.toggle).toHaveBeenCalledWith('POST /pets');
    expect(box('GET /pets').getAttribute('aria-label')).toBe('Test GET /pets');
  });

  it('can all be chosen or all left out with one click', () => {
    const calls = show();
    expect(text('openapi-preview')).toContain('4 of 4 operations chosen');
    fireEvent.click(el('openapi-select-all'));
    expect(calls.all).toHaveBeenLastCalledWith(true);
    fireEvent.click(el('openapi-select-none'));
    expect(calls.all).toHaveBeenLastCalledWith(false);
    cleanup();
    show(planFor(), { selected: new Set(['GET /pets']) });
    expect(text('openapi-preview')).toContain('1 of 4 operations chosen');
  });

  it('say why an operation has no case', () => {
    const plan = planFor({
      operations: [...OPERATIONS, { id: 'GET /old', method: 'GET', path: '/old', summary: null, tags: [], deprecated: true, requires_auth: false }],
      skipped: [{ operation: 'GET /old', reason: 'the operation is marked deprecated' }],
    });
    show(plan);
    expect(text('openapi-op-GET /old')).toContain('0 cases');
    expect(text('openapi-op-GET /old')).toContain('No case: the operation is marked deprecated.');
  });

  it('say that none of the asked-for kinds applies when no reason was given', () => {
    const plan = planFor({ operations: [...OPERATIONS, { id: 'GET /empty', method: 'GET', path: '/empty', summary: null, tags: [], deprecated: false, requires_auth: false }] });
    show(plan);
    expect(text('openapi-op-GET /empty')).toContain('No case: none of the asked-for kinds applies to it.');
  });
});

describe('the cases of an operation', () => {
  const rows = (op: string) => Array.from(el(`openapi-op-${op}`).querySelectorAll('tbody tr')).map((tr) => Array.from(tr.querySelectorAll('td')).map((td) => td.textContent ?? ''));

  it('give the kind, the case, the request and the answer it expects', () => {
    show();
    expect(rows('GET /pets')).toEqual([
      ['Valid request', 'a valid request gets a documented success', 'GET /fixture/pets?limit=1', '200 · body fits its schema'],
      ['Missing required', 'without the required query parameter limit', 'GET /fixture/pets', '400'],
      ['Wrong type', 'the parameter limit with a value of the wrong kind', 'GET /fixture/pets?limit=not-a-number', '400'],
    ]);
    expect(rows('GET /secure')[1]).toEqual(['Without a credential', 'without a credential it is refused', 'GET /fixture/secure', '401 or 403']);
    expect(rows('GET /pets/{petId}')[2]).toEqual(['Not found', 'a petId that is not there is a 404', 'GET /fixture/pets/999999999', '404']);
  });

  it('mark the ones that are not run, with the reason on hover, and only those', () => {
    show();
    const marks = Array.from(el('openapi-op-POST /pets').querySelectorAll('tbody tr')).map((tr) => tr.querySelector('[title]'));
    expect(marks).toHaveLength(4);
    for (const mark of marks) {
      expect(mark?.textContent).toBe('not run: changes data');
      expect(mark?.getAttribute('title')).toContain('POST changes data on the server');
    }
    expect(el('openapi-op-GET /pets').querySelector('tbody [title]')).toBeNull();
  });

  it('are open for an operation with a few, and shut for one with many, so the page stays short', () => {
    const many = planFor();
    const extra = Array.from({ length: 4 }, (_, i) => ({ ...many.cases[0], id: `GET /pets#EXTRA${i}` }));
    many.cases = [...many.cases, ...extra];
    show(many);
    expect((el('openapi-op-GET /pets') as HTMLDetailsElement).open).toBe(false);
    expect((el('openapi-op-GET /secure') as HTMLDetailsElement).open).toBe(true);
    expect((el('openapi-op-POST /pets') as HTMLDetailsElement).open).toBe(true);
  });

  it('name their table for a screen reader', () => {
    show();
    expect(el('openapi-op-GET /pets').querySelector('caption')?.textContent).toBe('Cases for GET /pets');
  });
});

describe('what the service says about the plan', () => {
  it('shows its notes', () => {
    show(planFor({ notes: ['GET /pets: no client-error response is documented, so no "missing required" case is made'] }));
    expect(text('openapi-notes')).toContain('no client-error response is documented');
    cleanup();
    show();
    expect(el('openapi-notes')).toBeNull();
  });

  it('names the variables the credentials need that the orchestrator does not have, one or several', () => {
    show(planFor({ missing_secrets: ['AIQA_SECRET_API_TOKEN'] }));
    expect(text('openapi-missing-secrets')).toContain('This variable is not set in the orchestrator’s environment: AIQA_SECRET_API_TOKEN.');
    expect(text('openapi-missing-secrets')).toContain('Add it to its .env');
    cleanup();
    show(planFor({ missing_secrets: ['AIQA_SECRET_A', 'AIQA_LOGIN_B'] }));
    expect(text('openapi-missing-secrets')).toContain('These variables are not set');
    expect(text('openapi-missing-secrets')).toContain('AIQA_SECRET_A, AIQA_LOGIN_B');
    expect(text('openapi-missing-secrets')).toContain('Add them to its .env');
    cleanup();
    show();
    expect(el('openapi-missing-secrets')).toBeNull();
  });
});

describe('the button that runs them', () => {
  it('says how many cases it runs and runs them', () => {
    const calls = show();
    const run = el('openapi-run') as HTMLButtonElement;
    expect(run.textContent).toBe('Run 8 cases');
    expect(run.disabled).toBe(false);
    expect(el('openapi-run-blocked')).toBeNull();
    fireEvent.click(run);
    expect(calls.run).toHaveBeenCalledTimes(1);
  });

  it('says one case in the singular', () => {
    const plan = planFor();
    plan.cases = plan.cases.filter((c) => c.id === 'GET /pets#HAPPY');
    show(plan, { selected: new Set(['GET /pets']) });
    expect(text('openapi-run')).toBe('Run 1 case');
  });

  it('cannot be pressed when nothing chosen has a case to run, and says why', () => {
    const calls = show(planFor(), { selected: new Set(['POST /pets']) });
    const run = el('openapi-run') as HTMLButtonElement;
    expect(run.disabled).toBe(true);
    expect(run.textContent).toBe('Run 0 cases');
    expect(text('openapi-run-blocked')).toBe('Choose an operation that has a case to run.');
    fireEvent.click(run);
    expect(calls.run).not.toHaveBeenCalled();
  });

  it('cannot be pressed while a variable the credentials name is not set, and says so', () => {
    show(planFor({ missing_secrets: ['AIQA_SECRET_X'] }));
    expect((el('openapi-run') as HTMLButtonElement).disabled).toBe(true);
    expect(text('openapi-run-blocked')).toBe('A variable the credentials name is not set.');
  });

  it('says it is starting while it does, and cannot be pressed again meanwhile', () => {
    const calls = show(planFor(), { running: true });
    const run = el('openapi-run') as HTMLButtonElement;
    expect(run.textContent).toBe('Starting…');
    expect(run.disabled).toBe(true);
    expect(run.querySelector('svg')?.getAttribute('class')).toContain('animate-spin');
    fireEvent.click(run);
    expect(calls.run).not.toHaveBeenCalled();
  });
});

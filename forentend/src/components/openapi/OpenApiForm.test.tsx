/**
 * Phase 8B — the form that says what to test: the document, where the API is, the kinds of case, the credentials (by variable name),
 * and whether requests that change data may be run.
 */
import React, { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { OpenApiForm } from './OpenApiForm';
import { emptyForm, formProblems, type OpenApiFormState } from './openApiModel';

afterEach(cleanup);

const el = (id: string) => document.getElementById(id) as HTMLElement;
const input = (id: string) => el(id) as HTMLInputElement;

/** The form with its own state, as the page holds it, and a record of what it was asked to do. */
function harness(start: Partial<OpenApiFormState> = {}, props: { showing?: boolean; disabled?: boolean } = {}) {
  const shown = vi.fn();
  const latest: { form: OpenApiFormState } = { form: { ...emptyForm(), ...start } };
  const Harness: React.FC = () => {
    const [form, setForm] = useState<OpenApiFormState>({ ...emptyForm(), ...start });
    latest.form = form;
    return (
      <OpenApiForm
        form={form}
        onChange={(next) => setForm(next)}
        problems={formProblems(form)}
        onShowCases={shown}
        showing={props.showing ?? false}
        disabled={props.disabled}
      />
    );
  };
  render(<Harness />);
  return { shown, form: () => latest.form };
}

describe('the document', () => {
  it('is pasted by default, in a box, and the address is not asked for', () => {
    harness();
    expect(el('openapi-spec')).toBeTruthy();
    expect(el('openapi-spec-url')).toBeNull();
    expect(el('openapi-source-paste').getAttribute('aria-pressed')).toBe('true');
    expect(el('openapi-source-address').getAttribute('aria-pressed')).toBe('false');
  });

  it('can be given by its address instead, and the text box goes away', () => {
    const { form } = harness();
    fireEvent.click(el('openapi-source-address'));
    expect(el('openapi-spec-url')).toBeTruthy();
    expect(el('openapi-spec')).toBeNull();
    expect(form().source).toBe('address');
    expect(el('openapi-source-address').getAttribute('aria-pressed')).toBe('true');
    expect(el('openapi-source-paste').getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(el('openapi-source-paste'));
    expect(el('openapi-spec')).toBeTruthy();
  });

  it('holds what is typed, in the box and in the address, and where the API is', () => {
    const { form } = harness();
    fireEvent.change(el('openapi-spec'), { target: { value: 'openapi: 3.0.0' } });
    expect(form().spec).toBe('openapi: 3.0.0');
    fireEvent.change(el('openapi-base-url'), { target: { value: 'https://api.example.com/v1' } });
    expect(form().baseUrl).toBe('https://api.example.com/v1');
    fireEvent.click(el('openapi-source-address'));
    fireEvent.change(el('openapi-spec-url'), { target: { value: 'https://x.test/openapi.json' } });
    expect(form().specUrl).toBe('https://x.test/openapi.json');
    expect(form().spec).toBe('openapi: 3.0.0');
  });
});

describe('the looks of the form', () => {
  it('marks the way the document is given that is chosen, and only that one', () => {
    harness();
    expect(el('openapi-source-paste').className).toContain('bg-violet-950/70 text-violet-200');
    expect(el('openapi-source-paste').className).not.toContain('text-slate-400');
    expect(el('openapi-source-address').className).toContain('text-slate-400 hover:bg-slate-800');
    expect(el('openapi-source-address').className).not.toContain('bg-violet-950/70');
    fireEvent.click(el('openapi-source-address'));
    expect(el('openapi-source-address').className).toContain('bg-violet-950/70 text-violet-200');
    expect(el('openapi-source-address').className).not.toContain('text-slate-400');
    expect(el('openapi-source-paste').className).toContain('text-slate-400 hover:bg-slate-800');
    expect(el('openapi-source-paste').className).not.toContain('bg-violet-950/70');
  });

  it('gives the box for the document room for a page of it, and no red lines under what is not prose', () => {
    harness();
    const box = el('openapi-spec') as HTMLTextAreaElement;
    expect(box.getAttribute('rows')).toBe('10');
    expect(box.getAttribute('spellcheck')).toBe('false');
  });
});

describe('the kinds of case', () => {
  it('are all chosen at first, each with its own name and hint', () => {
    harness();
    for (const kind of ['HAPPY', 'NO_AUTH', 'MISSING_REQUIRED', 'WRONG_TYPE', 'NOT_FOUND']) expect(input(`openapi-kind-${kind}`).checked, kind).toBe(true);
    expect(el('openapi-form').textContent).toContain('Valid request');
    expect(el('openapi-form').textContent).toContain('an id that is not there');
  });

  it('can be left out and put back, and the order is always the service’s own', () => {
    const { form } = harness();
    fireEvent.click(el('openapi-kind-NO_AUTH'));
    expect(form().kinds).toEqual(['HAPPY', 'MISSING_REQUIRED', 'WRONG_TYPE', 'NOT_FOUND']);
    fireEvent.click(el('openapi-kind-HAPPY'));
    expect(form().kinds).toEqual(['MISSING_REQUIRED', 'WRONG_TYPE', 'NOT_FOUND']);
    fireEvent.click(el('openapi-kind-NO_AUTH'));
    expect(form().kinds).toEqual(['NO_AUTH', 'MISSING_REQUIRED', 'WRONG_TYPE', 'NOT_FOUND']);
    fireEvent.click(el('openapi-kind-HAPPY'));
    expect(form().kinds).toEqual(['HAPPY', 'NO_AUTH', 'MISSING_REQUIRED', 'WRONG_TYPE', 'NOT_FOUND']);
    expect(input('openapi-kind-NO_AUTH').checked).toBe(true);
  });

  it('holds the most cases for one operation as a number', () => {
    const { form } = harness();
    expect(input('openapi-max-cases').value).toBe('8');
    fireEvent.change(el('openapi-max-cases'), { target: { value: '3' } });
    expect(form().maxCases).toBe(3);
  });
});

describe('the credentials', () => {
  it('start with none, and say what they are: the name of a variable, never the credential', () => {
    harness();
    expect(el('openapi-form').textContent).toContain('never stored, shown or sent anywhere else');
    expect(document.querySelectorAll('[id^="openapi-credential-header-"]')).toHaveLength(0);
  });

  it('are a header and a variable each, added, edited and removed', () => {
    const { form } = harness();
    fireEvent.click(el('openapi-add-credential'));
    fireEvent.click(el('openapi-add-credential'));
    const rows = form().credentials;
    expect(rows).toHaveLength(2);
    expect(rows.map((r) => r.header)).toEqual(['Authorization', 'Authorization']);
    expect(new Set(rows.map((r) => r.id)).size).toBe(2);
    const [first, second] = rows;
    fireEvent.change(el(`openapi-credential-variable-${first.id}`), { target: { value: 'AIQA_SECRET_API_TOKEN' } });
    fireEvent.change(el(`openapi-credential-header-${second.id}`), { target: { value: 'X-Api-Key' } });
    expect(form().credentials[0]).toMatchObject({ header: 'Authorization', variable: 'AIQA_SECRET_API_TOKEN' });
    expect(form().credentials[1]).toMatchObject({ header: 'X-Api-Key', variable: '' });
    expect(input(`openapi-credential-variable-${first.id}`).value).toBe('AIQA_SECRET_API_TOKEN');
    fireEvent.click(el(`openapi-credential-remove-${first.id}`));
    expect(form().credentials.map((r) => r.id)).toEqual([second.id]);
  });
});

describe('requests that change data', () => {
  it('are not run unless the box is ticked, and ticking it warns that they are really sent', () => {
    const { form } = harness();
    expect(input('openapi-unsafe').checked).toBe(false);
    expect(el('openapi-unsafe-warning')).toBeNull();
    fireEvent.click(el('openapi-unsafe'));
    expect(form().includeUnsafe).toBe(true);
    expect(el('openapi-unsafe-warning').textContent).toContain('really sent');
    expect(el('openapi-unsafe-warning').textContent).toContain('Use a test environment');
    fireEvent.click(el('openapi-unsafe'));
    expect(form().includeUnsafe).toBe(false);
    expect(el('openapi-unsafe-warning')).toBeNull();
  });
});

describe('asking for the cases', () => {
  it('lists what is wrong and does not let the button be pressed', () => {
    const { shown } = harness();
    expect(el('openapi-problems').textContent).toContain('Paste the OpenAPI document');
    expect((el('openapi-show-cases') as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(el('openapi-show-cases'));
    fireEvent.submit(el('openapi-form'));
    expect(shown).not.toHaveBeenCalled();
  });

  it('asks once the form has nothing wrong, by the button or by submitting the form', () => {
    const { shown } = harness({ spec: 'openapi: 3.0.0' });
    expect(el('openapi-problems')).toBeNull();
    const button = el('openapi-show-cases') as HTMLButtonElement;
    expect(button.disabled).toBe(false);
    expect(button.textContent).toBe('Show the cases');
    fireEvent.click(button);
    expect(shown).toHaveBeenCalledTimes(1);
    fireEvent.submit(el('openapi-form'));
    expect(shown).toHaveBeenCalledTimes(2);
  });

  it('says it is reading while it does, and does not ask again meanwhile', () => {
    const { shown } = harness({ spec: 'openapi: 3.0.0' }, { showing: true });
    const button = el('openapi-show-cases') as HTMLButtonElement;
    expect(button.textContent).toBe('Reading the document…');
    expect(button.disabled).toBe(true);
    fireEvent.submit(el('openapi-form'));
    expect(shown).not.toHaveBeenCalled();
  });

  it('is shut while the page is busy with a run', () => {
    const { shown } = harness({ spec: 'openapi: 3.0.0' }, { disabled: true });
    expect((el('openapi-show-cases') as HTMLButtonElement).disabled).toBe(true);
    expect(input('openapi-base-url').closest('fieldset')?.disabled).toBe(true);
    expect(input('openapi-unsafe').closest('fieldset')?.disabled).toBe(true);
    fireEvent.submit(el('openapi-form'));
    expect(shown).not.toHaveBeenCalled();
  });
});

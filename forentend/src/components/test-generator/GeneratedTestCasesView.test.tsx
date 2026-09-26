/**
 * Two focused, standalone additions to `GeneratedTestCasesView`
 * (which otherwise has no test coverage; testing the whole pre-existing
 * component is out of scope here). Rendered directly with plain
 * props/callbacks, same convention as `ApplicationSourceSection.test.tsx`
 * in this same folder.
 *
 * 1. The collapsible "Document Excerpt" toggle inside the Source
 *    Specification & Acceptance Criteria panel — before this, a long real
 *    document excerpt always rendered in full, pushing the rest of the
 *    panel far down the page. It now starts collapsed (just the label + an
 *    Expand control) and only renders the real text once expanded — the
 *    underlying `sourceRequirement` value itself is unchanged either way,
 *    this only toggles whether it's shown.
 * 2. The "Edit" button that opens `EditTestCaseModal` — this view only
 *    owns the modal's open/close state and wires its real `onSave` through
 *    to `onEditTestCase`; the modal's own field-editing/validation/error
 *    behavior is covered in `EditTestCaseModal.test.tsx`, not repeated
 *    here.
 */
import React from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GeneratedTestCasesView } from './GeneratedTestCasesView';
import type { GeneratedTestCase } from '../../types';
import type { CompilationOutcome } from '../../types/contract';
import type { GroundingCrawl } from './CrawlGroundingSection';

const LONG_EXCERPT =
  'Focus particularly on these test categories where applicable: Functional, Non-Functional. '.repeat(20);

function testCase(overrides: Partial<GeneratedTestCase> = {}): GeneratedTestCase {
  return {
    id: 'AI-001',
    title: 'Verify that notifyOnFailure sends an email with the correct parameters',
    description: 'A real generated test case.',
    category: 'Functional',
    priority: 'HIGH',
    severity: 'MAJOR',
    sourceDocument: 'BRD + PRD + FRD + SRD',
    requirementReference: '§3.1 Spec',
    sourceRequirement: LONG_EXCERPT,
    preconditions: [],
    steps: [{ stepNumber: 1, action: 'Trigger a failure.', expectedResult: 'An email is sent.' }],
    assertions: [],
    riskFactor: 'MEDIUM',
    status: 'PENDING_REVIEW',
    suggestedTags: [],
    ...overrides,
  };
}

function noop() {}

describe('GeneratedTestCasesView — collapsible Document Excerpt', () => {
  it('starts collapsed — the real excerpt text is not shown until Expand is clicked', () => {
    render(
      <GeneratedTestCasesView
        testCases={[testCase()]}
        onApproveTestCase={noop}
        onRejectTestCase={noop}
        onApproveAll={noop}
        onOpenExecutionHub={noop}
        onNavigateToSources={noop}
      />,
    );

    expect(screen.getByText('Document Excerpt')).toBeTruthy();
    expect(screen.queryByText(new RegExp(LONG_EXCERPT.slice(0, 40)))).toBeNull();
    expect(document.getElementById('btn-toggle-source-excerpt')!.textContent).toContain('Expand');
  });

  it('clicking Expand reveals the real excerpt text and flips the control to Collapse; clicking again hides it', () => {
    render(
      <GeneratedTestCasesView
        testCases={[testCase()]}
        onApproveTestCase={noop}
        onRejectTestCase={noop}
        onApproveAll={noop}
        onOpenExecutionHub={noop}
        onNavigateToSources={noop}
      />,
    );

    const toggle = document.getElementById('btn-toggle-source-excerpt')!;
    act(() => {
      fireEvent.click(toggle);
    });

    expect(screen.getByText(new RegExp(LONG_EXCERPT.slice(0, 40)))).toBeTruthy();
    expect(toggle.textContent).toContain('Collapse');

    act(() => {
      fireEvent.click(toggle);
    });
    expect(screen.queryByText(new RegExp(LONG_EXCERPT.slice(0, 40)))).toBeNull();
    expect(toggle.textContent).toContain('Expand');
  });

  it('a test case with no real sourceRequirement shows no excerpt block at all — never a fabricated placeholder excerpt', () => {
    render(
      <GeneratedTestCasesView
        testCases={[testCase({ sourceRequirement: undefined })]}
        onApproveTestCase={noop}
        onRejectTestCase={noop}
        onApproveAll={noop}
        onOpenExecutionHub={noop}
        onNavigateToSources={noop}
      />,
    );

    expect(screen.queryByText('Document Excerpt')).toBeNull();
    expect(document.getElementById('btn-toggle-source-excerpt')).toBeNull();
  });
});

describe('GeneratedTestCasesView — Edit Test Case wiring', () => {
  it('shows no Edit control at all when the caller provides no onEditTestCase — never a dead button', () => {
    render(
      <GeneratedTestCasesView
        testCases={[testCase()]}
        onApproveTestCase={noop}
        onRejectTestCase={noop}
        onApproveAll={noop}
        onOpenExecutionHub={noop}
        onNavigateToSources={noop}
      />,
    );
    expect(document.getElementById('btn-edit-test-case')).toBeNull();
  });

  it('Edit opens the real modal pre-filled with this case, and Save calls the real onEditTestCase with this case\'s real id', async () => {
    const onEditTestCase = vi.fn().mockResolvedValue(undefined);
    render(
      <GeneratedTestCasesView
        testCases={[testCase()]}
        onApproveTestCase={noop}
        onRejectTestCase={noop}
        onApproveAll={noop}
        onOpenExecutionHub={noop}
        onNavigateToSources={noop}
        onEditTestCase={onEditTestCase}
      />,
    );

    await act(async () => {
      fireEvent.click(document.getElementById('btn-edit-test-case')!);
    });

    expect(document.getElementById('edit-test-case-modal')).toBeTruthy();
    expect((document.getElementById('edit-tc-title') as HTMLInputElement).value).toBe(testCase().title);

    await act(async () => {
      fireEvent.click(document.getElementById('btn-save-test-case-edit')!);
    });

    expect(onEditTestCase).toHaveBeenCalledTimes(1);
    expect(onEditTestCase.mock.calls[0][0]).toBe('AI-001');

    // The modal closes itself on a successful save.
    expect(document.getElementById('edit-test-case-modal')).toBeNull();
  });
});

describe('GeneratedTestCasesView — Phase 5: compile against a crawl', () => {
  const CRAWL: GroundingCrawl = {
    runId: 'crawl-run-1',
    roles: [
      { name: 'anonymous', loggedIn: true },
      { name: 'admin', loggedIn: true },
      { name: 'auditor', loggedIn: false },
    ],
  };

  const READY: CompilationOutcome = {
    status: 'READY',
    test_case_id: 'AI-001',
    execution_type: 'BROWSER',
    target: 'https://app.example.com/dashboard',
    actions: [
      { type: 'NAVIGATE', url: 'https://app.example.com/login' },
      { type: 'NAVIGATE', url: 'https://app.example.com/dashboard' },
    ],
    compiler: 'AI',
    diagnostics: [],
    provider: 'local',
    model: 'qwen',
  };

  const GROUNDED_READY: CompilationOutcome = {
    ...READY,
    grounding: {
      role: 'admin',
      pages_in_crawl: 3,
      pages_offered: 2,
      elements_offered: 47,
      login_steps: 4,
      secrets: ['AIQA_LOGIN_ADMIN_PASSWORD', 'AIQA_LOGIN_ADMIN_USERNAME'],
    },
  };

  function renderApproved(props: Partial<React.ComponentProps<typeof GeneratedTestCasesView>> = {}) {
    const onCompilePreview = vi.fn();
    const onHandoffTestCase = vi.fn();
    render(
      <GeneratedTestCasesView
        testCases={[testCase({ status: 'APPROVED' })]}
        hasPersisted
        onApproveTestCase={noop}
        onRejectTestCase={noop}
        onApproveAll={noop}
        onOpenExecutionHub={noop}
        onNavigateToSources={noop}
        onHandoffTestCase={onHandoffTestCase}
        onCompilePreview={onCompilePreview}
        {...props}
      />,
    );
    fireEvent.change(document.getElementById('handoff-executor-select')!, { target: { value: '__compile__' } });
    return { onCompilePreview, onHandoffTestCase };
  }

  const toggle = () => document.getElementById('compile-grounded-toggle') as HTMLInputElement | null;
  const roleSelect = () => document.getElementById('compile-role-select') as HTMLSelectElement | null;
  const previewButton = () => document.getElementById('btn-preview-compilation') as HTMLButtonElement;
  const handoffButton = () => document.getElementById('btn-handoff-test-case') as HTMLButtonElement;

  it('offers no grounding without a crawl, only a hint to crawl the application', () => {
    renderApproved();
    expect(toggle()).toBeNull();
    expect(document.getElementById('compile-grounding-controls')!.textContent).toContain('Crawl the application');
  });

  it('with a crawl it is off by default: the preview asks for nothing beyond the case, exactly as before', () => {
    const { onCompilePreview } = renderApproved({ groundingCrawl: CRAWL });
    expect(toggle()!.checked).toBe(false);
    expect(roleSelect()).toBeNull();

    fireEvent.click(previewButton());
    expect(onCompilePreview).toHaveBeenCalledWith('AI-001', undefined);
  });

  it('ticking it and choosing a role makes the preview ask for that crawl and that role', () => {
    const { onCompilePreview } = renderApproved({ groundingCrawl: CRAWL });
    fireEvent.click(toggle()!);
    fireEvent.change(roleSelect()!, { target: { value: 'admin' } });

    fireEvent.click(previewButton());
    expect(onCompilePreview).toHaveBeenCalledWith('AI-001', { crawl_run_id: 'crawl-run-1', role: 'admin' });
  });

  it('with no role chosen it asks for the crawl alone, and the roles listed are the crawl\'s own (never "anonymous", which is No login)', () => {
    const { onCompilePreview } = renderApproved({ groundingCrawl: CRAWL });
    fireEvent.click(toggle()!);

    const options = Array.from(roleSelect()!.options).map((o) => [o.value, o.textContent]);
    expect(options).toEqual([
      ['', 'No login'],
      ['admin', 'admin'],
      ['auditor', 'auditor (login failed)'],
    ]);

    fireEvent.click(previewButton());
    const [, grounding] = onCompilePreview.mock.calls[0];
    expect(grounding).toEqual({ crawl_run_id: 'crawl-run-1' });
    expect('role' in grounding).toBe(false);
  });

  it('a READY preview lets the handoff go ahead only for what it was previewed with', () => {
    renderApproved({ groundingCrawl: CRAWL, compilePreview: GROUNDED_READY, compilePreviewForId: 'AI-001' });

    fireEvent.click(toggle()!);
    fireEvent.change(roleSelect()!, { target: { value: 'admin' } });
    // A READY preview exists, but it was not made for this choice.
    expect(handoffButton().disabled).toBe(true);

    fireEvent.click(previewButton());
    expect(handoffButton().disabled).toBe(false);

    // Any change to what is compiled against needs a new preview.
    fireEvent.change(roleSelect()!, { target: { value: '' } });
    expect(handoffButton().disabled).toBe(true);
    fireEvent.click(previewButton());
    expect(handoffButton().disabled).toBe(false);
    fireEvent.click(toggle()!);
    expect(handoffButton().disabled).toBe(true);
  });

  it('the handoff carries the grounding it was previewed with, and nothing extra when it was not grounded', () => {
    const grounded = renderApproved({ groundingCrawl: CRAWL, compilePreview: GROUNDED_READY, compilePreviewForId: 'AI-001' });
    fireEvent.click(toggle()!);
    fireEvent.change(roleSelect()!, { target: { value: 'admin' } });
    fireEvent.click(previewButton());
    fireEvent.click(handoffButton());
    expect(grounded.onHandoffTestCase).toHaveBeenCalledWith('AI-001', {
      mode: 'compile',
      grounding: { crawl_run_id: 'crawl-run-1', role: 'admin' },
    });
  });

  it('an ungrounded handoff is the plain compile choice, with no grounding key at all', () => {
    const plain = renderApproved({ groundingCrawl: CRAWL, compilePreview: READY, compilePreviewForId: 'AI-001' });
    fireEvent.click(previewButton());
    fireEvent.click(handoffButton());
    expect(plain.onHandoffTestCase).toHaveBeenCalledTimes(1);
    const [, choice] = plain.onHandoffTestCase.mock.calls[0];
    expect(choice).toEqual({ mode: 'compile' });
    expect('grounding' in choice).toBe(false);
  });

  it('a grounded READY preview says what it was grounded in and names the variables it reads', () => {
    renderApproved({ groundingCrawl: CRAWL, compilePreview: GROUNDED_READY, compilePreviewForId: 'AI-001' });
    const note = document.getElementById('compile-grounding-note')!.textContent!;
    expect(note).toContain('signing in as admin first');
    expect(note).toContain('47 elements on 2 of 3 pages');
    const secrets = document.getElementById('compile-secrets-note')!.textContent!;
    expect(secrets).toContain('AIQA_LOGIN_ADMIN_PASSWORD, AIQA_LOGIN_ADMIN_USERNAME');
    expect(secrets).toContain('must be set there');
  });

  it('a preview that was not grounded shows neither note', () => {
    renderApproved({ groundingCrawl: CRAWL, compilePreview: READY, compilePreviewForId: 'AI-001' });
    expect(document.getElementById('compile-grounding-note')).toBeNull();
    expect(document.getElementById('compile-secrets-note')).toBeNull();
  });

  it('a case compiled without signing in says so and lists no variables', () => {
    const anonymous: CompilationOutcome = {
      ...READY,
      grounding: { role: null, pages_in_crawl: 3, pages_offered: 1, elements_offered: 9, login_steps: 0, secrets: [] },
    };
    renderApproved({ groundingCrawl: CRAWL, compilePreview: anonymous, compilePreviewForId: 'AI-001' });
    expect(document.getElementById('compile-grounding-note')!.textContent).toContain('without signing in');
    expect(document.getElementById('compile-secrets-note')).toBeNull();
  });

  it('a grounded NOT_EXECUTABLE preview lists the real reasons and keeps the handoff off', () => {
    const refused: CompilationOutcome = {
      status: 'NOT_EXECUTABLE',
      reasons: ['step 2 (CLICK) uses the selector "#nope", which the crawl never found on any page'],
      grounding: { role: 'admin', pages_in_crawl: 3, pages_offered: 2, elements_offered: 47, login_steps: 4, secrets: [] },
    };
    renderApproved({ groundingCrawl: CRAWL, compilePreview: refused, compilePreviewForId: 'AI-001' });
    fireEvent.click(toggle()!);
    fireEvent.click(previewButton());
    expect(document.getElementById('compile-preview-panel')!.textContent).toContain('the crawl never found on any page');
    expect(handoffButton().disabled).toBe(true);
  });
});

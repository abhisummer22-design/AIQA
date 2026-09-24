/**
 * Phase 3 — the opt-in "generate from numbered requirements" section.
 *
 * `fetch` is mocked only at the transport boundary, like every other test in
 * this app: the plan and the requirement list are the backend's own answers,
 * and everything asserted here is what the section makes of them.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RequirementGenerationSection } from './RequirementGenerationSection';
import { setApiKey } from '../../services/aiqaApi';
import type { UploadedRequirementDoc } from '../../types';
import type { GenerateAiPlanResponse, GenerateAiSkillInput, RequirementRecord } from '../../types/contract';

const PROJECT_ID = 'proj-req-gen';
const DOC_FRD = '11111111-1111-4111-8111-111111111111';
const DOC_SRD = '22222222-2222-4222-8222-222222222222';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

function doc(overrides: Partial<UploadedRequirementDoc> = {}): UploadedRequirementDoc {
  return {
    slotId: 'FRD',
    label: '3. FRD',
    fullName: 'Functional Requirements Document',
    fileName: 'FRD.docx',
    fileSize: 1000,
    fileType: 'DOCX',
    uploadedAt: '2026-09-24T00:00:00Z',
    status: 'READY',
    requirementsDocumentId: DOC_FRD,
    requirementsCount: 140,
    ...overrides,
  };
}

const SKILLS: GenerateAiSkillInput[] = [
  { id: 'functional', name: 'Functional Testing' },
  { id: 'security', name: 'Security Testing' },
];

function planResponse(overrides: Partial<GenerateAiPlanResponse> = {}): GenerateAiPlanResponse {
  return {
    requirements_total: 140,
    by_kind: { functional: 100, security: 40 },
    offered: 140,
    not_offered_keys: [],
    calls: 6,
    max_calls: 150,
    within_limit: true,
    skills: [
      { skill_name: 'Functional Testing', wave: 'Base', requirements: 140, batches: 4 },
      { skill_name: 'Security Testing', wave: 'NonFunctional', requirements: 40, batches: 2 },
    ],
    budget_chars: 12000,
    provider_name: 'Groq',
    model: 'llama-3.3-70b',
    prompt_window_tokens: 8000,
    estimated_prompt_tokens: 25000,
    estimated_completion_tokens: 9000,
    ...overrides,
  };
}

function requirement(n: number, overrides: Partial<RequirementRecord> = {}): RequirementRecord {
  return {
    document_id: DOC_FRD,
    key: `FR-${String(n).padStart(3, '0')}`,
    ordinal: n,
    section: '3.2 Login',
    kind: 'functional',
    text: `The system shall do thing number ${n}.`,
    ...overrides,
  };
}

interface Requests {
  plans: Array<{ url: string; body: { skills: unknown[]; requirement_document_ids: string[] } }>;
  lists: string[];
}

function stubBackend(opts: {
  plan?: () => Response;
  requirements?: RequirementRecord[];
  listStatus?: number;
} = {}): Requests {
  const seen: Requests = { plans: [], lists: [] };
  vi.stubGlobal(
    'fetch',
    vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.includes('/generate-ai/plan')) {
        seen.plans.push({ url, body: JSON.parse(String(init?.body)) });
        return opts.plan ? opts.plan() : jsonResponse(200, planResponse());
      }
      if (url.includes('/requirements')) {
        seen.lists.push(url);
        if (opts.listStatus && opts.listStatus !== 200) {
          return jsonResponse(opts.listStatus, { error: { code: 'X', message: 'nope', details: {}, request_id: 'r' } });
        }
        return jsonResponse(200, { documents: [], requirements: opts.requirements ?? [] });
      }
      throw new Error(`unexpected request: ${url}`);
    }),
  );
  return seen;
}

function Harness({
  documents,
  skills = SKILLS,
  initiallyEnabled = false,
  disabled = false,
}: {
  documents: UploadedRequirementDoc[];
  skills?: GenerateAiSkillInput[];
  initiallyEnabled?: boolean;
  disabled?: boolean;
}) {
  const [enabled, setEnabled] = React.useState(initiallyEnabled);
  return (
    <RequirementGenerationSection
      projectId={PROJECT_ID}
      enabled={enabled}
      onEnabledChange={setEnabled}
      documents={documents}
      skills={skills}
      disabled={disabled}
    />
  );
}

const toggle = () => screen.getByLabelText('Generate from the requirements of my uploaded documents') as HTMLInputElement;

beforeEach(() => {
  vi.restoreAllMocks();
  setApiKey('aiqa_reqgen0000000000000');
});

afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
});

describe('RequirementGenerationSection — opting in', () => {
  it('is off by default and asks the backend for nothing until it is turned on', async () => {
    const seen = stubBackend();
    render(<Harness documents={[doc()]} />);

    expect(toggle().checked).toBe(false);
    expect(document.getElementById('requirement-plan')).toBeNull();
    await new Promise((r) => setTimeout(r, 600));
    expect(seen.plans).toHaveLength(0);
  });

  it('cannot be turned on with no document, and says why', () => {
    const seen = stubBackend();
    render(<Harness documents={[]} />);

    expect(toggle().disabled).toBe(true);
    expect(screen.getByText(/Upload a document first/)).toBeTruthy();
    expect(seen.plans).toHaveLength(0);
  });

  it('cannot be turned on when no uploaded document has requirements read from it, and says documents will be pasted as usual', () => {
    render(<Harness documents={[doc({ requirementsDocumentId: undefined, requirementsCount: undefined })]} />);

    expect(toggle().disabled).toBe(true);
    expect(screen.getByText(/No uploaded document has requirements to generate from/)).toBeTruthy();
    expect(screen.getByText(/documents will be pasted as usual/)).toBeTruthy();
  });

  it('shows the option off while no document has requirements, and finds it on again when one does', () => {
    stubBackend();
    const { rerender } = render(<Harness documents={[doc()]} initiallyEnabled />);
    expect(toggle().checked).toBe(true);

    rerender(<Harness documents={[]} initiallyEnabled />);
    expect(toggle().checked).toBe(false);
    expect(toggle().disabled).toBe(true);

    rerender(<Harness documents={[doc()]} initiallyEnabled />);
    expect(toggle().checked).toBe(true);
  });

  it('is disabled while a generation is running', () => {
    render(<Harness documents={[doc()]} disabled />);
    expect(toggle().disabled).toBe(true);
  });

  it('turning it on names the documents, and the requirements they hold, at once', async () => {
    stubBackend();
    render(<Harness documents={[doc(), doc({ slotId: 'SRD', requirementsDocumentId: DOC_SRD, requirementsCount: 60 })]} />);

    fireEvent.click(toggle());

    expect(await screen.findByText('2 documents · 200 requirements')).toBeTruthy();
  });
});

describe('RequirementGenerationSection — the plan', () => {
  it('asks for the plan of exactly these documents and skills, and shows calls, per-skill batches, where the calls go and a rough size', async () => {
    const seen = stubBackend();
    render(<Harness documents={[doc()]} />);

    fireEvent.click(toggle());

    const plan = await screen.findByText(/140 requirements → 6 AI calls \(one request may make at most 150\)/, {}, { timeout: 3000 });
    expect(seen.plans).toHaveLength(1);
    expect(seen.plans[0].url).toBe(`/api/v1/projects/${PROJECT_ID}/test-cases/generate-ai/plan`);
    expect(seen.plans[0].body.requirement_document_ids).toEqual([DOC_FRD]);
    expect(seen.plans[0].body.skills).toEqual(SKILLS);

    const box = plan.closest('#requirement-plan') as HTMLElement;
    expect(within(box).getByText('Functional Testing').closest('li')!.textContent).toContain('140 requirements · 4 batches');
    expect(within(box).getByText('Security Testing').closest('li')!.textContent).toContain('40 requirements · 2 batches');
    expect(box.textContent).toContain('Calls go to Groq / llama-3.3-70b · about 12,000 characters of requirements per call (window 8,000 tokens)');
    expect(box.textContent).toContain('about 25,000 tokens in and 9,000 out across all calls');
  });

  it('explains a small window, and does not when the window is large', async () => {
    stubBackend({ plan: () => jsonResponse(200, planResponse({ prompt_window_tokens: 4096, provider_name: 'Ollama' })) });
    const { unmount } = render(<Harness documents={[doc()]} />);
    fireEvent.click(toggle());
    expect(await screen.findByText(/This model's window is small/, {}, { timeout: 3000 })).toBeTruthy();
    expect(screen.getByText(/OLLAMA_NUM_CTX/)).toBeTruthy();
    unmount();

    stubBackend({ plan: () => jsonResponse(200, planResponse({ prompt_window_tokens: 32768 })) });
    render(<Harness documents={[doc()]} />);
    fireEvent.click(toggle());
    await screen.findByText(/140 requirements → 6 AI calls/, {}, { timeout: 3000 });
    expect(screen.queryByText(/This model's window is small/)).toBeNull();
  });

  it('names the requirements no selected skill would be shown, with a count of the ones not named', async () => {
    const keys = Array.from({ length: 10 }, (_, i) => `NFR-${i + 1}`);
    stubBackend({ plan: () => jsonResponse(200, planResponse({ offered: 130, not_offered_keys: keys })) });
    render(<Harness documents={[doc()]} />);

    fireEvent.click(toggle());

    const warning = await screen.findByText(/10 requirements are of a kind none of the selected skills tests/, {}, { timeout: 3000 });
    expect(warning.textContent).toContain('NFR-1, NFR-2, NFR-3, NFR-4, NFR-5, NFR-6, NFR-7, NFR-8 and 2 more');
    expect(warning.textContent).toContain('Select a skill that covers them.');
  });

  it('says nothing about uncovered requirements when every one would be shown to a skill', async () => {
    stubBackend();
    render(<Harness documents={[doc()]} />);
    fireEvent.click(toggle());
    await screen.findByText(/140 requirements → 6 AI calls/, {}, { timeout: 3000 });
    expect(screen.queryByText(/of a kind none of the selected skills tests/)).toBeNull();
  });

  it('warns when the request would make more calls than one request may', async () => {
    stubBackend({ plan: () => jsonResponse(200, planResponse({ calls: 212, within_limit: false })) });
    render(<Harness documents={[doc()]} />);

    fireEvent.click(toggle());

    const warning = await screen.findByText(/This would take 212 AI calls, more than the 150 one request may make/, {}, { timeout: 3000 });
    expect(warning.textContent).toContain('Select fewer skills or documents.');
  });

  it('says so when no AI provider can serve generation, instead of inventing one', async () => {
    stubBackend({ plan: () => jsonResponse(200, planResponse({ provider_name: null, model: null, prompt_window_tokens: null })) });
    render(<Harness documents={[doc()]} />);

    fireEvent.click(toggle());

    expect(await screen.findByText('No AI provider can serve generation right now.', {}, { timeout: 3000 })).toBeTruthy();
  });

  it('shows the backend\'s own reason when the plan cannot be worked out, never a made-up plan', async () => {
    stubBackend({
      plan: () => jsonResponse(403, { error: { code: 'FORBIDDEN', message: "role 'viewer' cannot perform this action", details: {}, request_id: 'r' } }),
    });
    render(<Harness documents={[doc()]} />);

    fireEvent.click(toggle());

    expect(await screen.findByText(/Could not work out the plan:/, {}, { timeout: 3000 })).toBeTruthy();
    expect(document.getElementById('requirement-plan')).toBeNull();
  });

  it('plans again, once, when the selected skills change — not on every keystroke of a change in progress', async () => {
    vi.useFakeTimers();
    const seen = stubBackend();
    const { rerender } = render(<Harness documents={[doc()]} initiallyEnabled />);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(399);
    });
    expect(seen.plans).toHaveLength(0);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(seen.plans).toHaveLength(1);

    const added = [...SKILLS, { id: 'edge', name: 'Edge Case Analysis' }];
    const more = [...added, { id: 'perf', name: 'Performance Testing' }];
    rerender(<Harness documents={[doc()]} initiallyEnabled skills={added} />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(200);
    });
    rerender(<Harness documents={[doc()]} initiallyEnabled skills={more} />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });

    expect(seen.plans).toHaveLength(2);
    expect(seen.plans[1].body.skills).toEqual(more);
  });

  it('plans for the documents that have requirements, and names the ones left out instead of quietly dropping them', async () => {
    const seen = stubBackend();
    render(
      <Harness
        documents={[
          doc(),
          doc({ slotId: 'BRD', fileName: 'business.doc', requirementsDocumentId: undefined, requirementsCount: undefined }),
        ]}
      />,
    );

    fireEvent.click(toggle());

    await screen.findByText(/140 requirements → 6 AI calls/, {}, { timeout: 3000 });
    expect(seen.plans[0].body.requirement_document_ids).toEqual([DOC_FRD]);
    expect(screen.getByText(/Left out while this is on/).textContent).toContain('BRD (business.doc)');
  });
});

describe('RequirementGenerationSection — seeing the requirements', () => {
  async function openList(requirements: RequirementRecord[], documents = [doc()]) {
    const seen = stubBackend({ requirements });
    render(<Harness documents={documents} initiallyEnabled />);
    await screen.findByText(/requirements → 6 AI calls/, {}, { timeout: 3000 });
    fireEvent.click(screen.getByRole('button', { name: /Show the requirements/ }));
    await screen.findByLabelText('Search the requirements');
    return seen;
  }

  it('reads the requirements of the documents in use only when asked, and shows each with its id, kind, section and text', async () => {
    const seen = await openList([requirement(1), requirement(2, { kind: 'security', section: '4.1 Access' })]);

    expect(seen.lists).toHaveLength(1);
    const query = new URL(seen.lists[0], 'http://localhost');
    expect(query.pathname).toBe(`/api/v1/projects/${PROJECT_ID}/requirements`);
    expect(query.searchParams.get('document_ids')).toBe(DOC_FRD);

    const list = document.getElementById('requirement-list') as HTMLElement;
    const second = within(list).getByText('FR-002').closest('li') as HTMLElement;
    expect(second.textContent).toContain('security');
    expect(second.textContent).toContain('4.1 Access');
    expect(second.textContent).toContain('The system shall do thing number 2.');
    expect(within(list).getByText('2 requirements.')).toBeTruthy();
  });

  it('does not read them at all while the list stays closed', async () => {
    const seen = stubBackend({ requirements: [requirement(1)] });
    render(<Harness documents={[doc()]} initiallyEnabled />);
    await screen.findByText(/requirements → 6 AI calls/, {}, { timeout: 3000 });
    expect(seen.lists).toHaveLength(0);
  });

  it('narrows by what is typed — id, kind, section or text — and says how many match', async () => {
    await openList([
      requirement(1),
      requirement(2, { kind: 'security', section: '4.1 Access', text: 'Passwords are hashed.' }),
      requirement(3, { text: 'Lockout after five attempts.' }),
    ]);
    const box = screen.getByLabelText('Search the requirements');

    fireEvent.change(box, { target: { value: 'security' } });
    expect(screen.queryByText('FR-001')).toBeNull();
    expect(screen.getByText('FR-002')).toBeTruthy();
    expect(screen.getByText('1 requirement match.')).toBeTruthy();

    fireEvent.change(box, { target: { value: 'lockout' } });
    expect(screen.getByText('FR-003')).toBeTruthy();
    expect(screen.queryByText('FR-002')).toBeNull();

    fireEvent.change(box, { target: { value: 'nothing like this' } });
    expect(screen.getByText('0 requirements match.')).toBeTruthy();
  });

  it('shows the first 200 and says how many more there are, never a silently cut list', async () => {
    const many = Array.from({ length: 250 }, (_, i) => requirement(i + 1));
    await openList(many);

    const list = document.getElementById('requirement-list') as HTMLElement;
    expect(within(list).getAllByRole('listitem')).toHaveLength(200);
    expect(within(list).getByText('Showing the first 200 of 250 matching requirements — search to narrow them.')).toBeTruthy();
  });

  it('tells apart which document a requirement is from when more than one is in use', async () => {
    await openList(
      [requirement(1), requirement(2, { document_id: DOC_SRD, key: 'SRD-001' })],
      [doc(), doc({ slotId: 'SRD', requirementsDocumentId: DOC_SRD, requirementsCount: 1 })],
    );

    const list = document.getElementById('requirement-list') as HTMLElement;
    expect((within(list).getByText('SRD-001').closest('li') as HTMLElement).textContent).toContain('SRD');
  });

  it('reports a failure to read them, rather than an empty list', async () => {
    stubBackend({ listStatus: 500 });
    render(<Harness documents={[doc()]} initiallyEnabled />);
    await screen.findByText(/requirements → 6 AI calls/, {}, { timeout: 3000 });

    fireEvent.click(screen.getByRole('button', { name: /Show the requirements/ }));

    // The backend's own message (`nope` above), not an empty list or a spinner.
    expect(await screen.findByText('nope')).toBeTruthy();
    expect(screen.queryByLabelText('Search the requirements')).toBeNull();
    expect(screen.queryByText(/Reading the requirements/)).toBeNull();
  });

  it('hides the list again on a second click', async () => {
    await openList([requirement(1)]);
    fireEvent.click(screen.getByRole('button', { name: /Hide the requirements/ }));
    expect(document.getElementById('requirement-list')).toBeNull();
  });
});

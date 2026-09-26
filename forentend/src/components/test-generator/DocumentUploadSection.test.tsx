/**
 * Truthfulness Cleanup — real Product Documentation content extraction,
 * plus the later BRD (Business Requirements Document) slot addition.
 *
 * Before the content-extraction work, `handleFileUpload` ran a
 * fixed-duration `setInterval` fake progress ramp ("Simulate real
 * asynchronous file ingestion") and never read a single byte of the
 * uploaded file — it only ever recorded the file's name/size/type. The
 * first describe block below proves the real replacement: a real
 * `aiqaApi.documents.extractText` call is made with the file's real,
 * base64-encoded bytes, and the UI renders exactly what that real
 * response says — never a fabricated success, never a hidden failure.
 * `fetch` is mocked only at the transport boundary, same convention as
 * every other test in this app.
 *
 * The second describe block covers the later BRD slot addition — a
 * focused, standalone rendering check (BRD joins PRD/FRD/SRD/Additional
 * as the new first slot). It renders `DocumentUploadSection` directly
 * with `projectId={null}` since none of its cases exercise the real
 * extraction request path above; they only check what's on screen.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DocumentUploadSection } from './DocumentUploadSection';
import { setApiKey } from '../../services/aiqaApi';
import type { RequirementDocType, UploadedRequirementDoc } from '../../types';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const RAW_KEY = 'aiqa_docupload00000000000000';
const PROJECT_ID = 'proj-doc-upload-1';

function emptyDocs(): UploadedRequirementDoc[] {
  return [];
}

function Harness({ initial, projectId = PROJECT_ID }: { initial?: UploadedRequirementDoc[]; projectId?: string | null }) {
  const [docs, setDocs] = React.useState<UploadedRequirementDoc[]>(initial ?? emptyDocs());
  return <DocumentUploadSection documents={docs} onDocumentsChange={setDocs} projectId={projectId} />;
}

function prdFileInput(): HTMLInputElement {
  return document.querySelector('#doc-slot-prd input[type=file]') as HTMLInputElement;
}

async function dropFile(file: File) {
  const input = prdFileInput();
  await act(async () => {
    fireEvent.change(input, { target: { files: [file] } });
  });
}

beforeEach(() => {
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('DocumentUploadSection — real content extraction', () => {
  it('a real successful extraction shows the real character count, never a fabricated one', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      expect(url).toContain('/documents/extract-text');
      const parsed = JSON.parse(String(init?.body));
      expect(parsed.file_name).toBe('BRD.md');
      // The real, exact base64 of "Real requirement content." — proves the
      // real file bytes were sent, not a placeholder.
      expect(parsed.content_base64).toBe(btoa('Real requirement content.'));
      return jsonResponse(200, { text: 'Real requirement content.', char_count: 26, truncated: false });
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    const file = new File(['Real requirement content.'], 'BRD.md', { type: 'text/markdown' });
    await dropFile(file);

    await waitFor(() => expect(screen.getByText(/26 chars of real content analyzed/)).toBeTruthy());
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('a truncated real extraction discloses the truncation, not a silent partial result', async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      jsonResponse(200, { text: 'x'.repeat(20000), char_count: 20000, truncated: true })
    );
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    await dropFile(new File(['content'], 'FRD.txt', { type: 'text/plain' }));

    await waitFor(() => expect(screen.getByText(/20,000 chars of real content analyzed \(truncated\)/)).toBeTruthy());
  });

  it('a real extraction failure (e.g. unsupported legacy .doc) is shown honestly, never as a fake success', async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      jsonResponse(400, {
        error: {
          code: 'VALIDATION_ERROR',
          message: 'legacy .doc (pre-2007 Word binary format) is not supported for content extraction.',
          details: {},
          request_id: 'req_1',
        },
      })
    );
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    await dropFile(new File(['binary'], 'RTM.doc', { type: 'application/msword' }));

    await waitFor(() =>
      expect(screen.getByText(/Content not analyzed: legacy \.doc/)).toBeTruthy()
    );
    // The file is still genuinely attached (shown as Ready) even though its
    // content could not be analyzed — never silently dropped.
    expect(screen.getByText('RTM.doc')).toBeTruthy();
    expect(screen.queryByText(/chars of real content analyzed/)).toBeNull();
  });

  it('a real network failure during extraction is disclosed, never silently swallowed', async () => {
    const fetchMock = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    await dropFile(new File(['content'], 'SRD.txt', { type: 'text/plain' }));

    await waitFor(() => expect(screen.getByText(/Content not analyzed/)).toBeTruthy());
  });

  it('with no project selected, the file is attached but no extraction request is ever sent', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness projectId={null} />);

    await dropFile(new File(['content'], 'PRD.txt', { type: 'text/plain' }));

    await waitFor(() =>
      expect(screen.getByText(/Select a project before its real content can be analyzed/)).toBeTruthy()
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('an unsupported file extension is rejected before any request is made', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    await dropFile(new File(['data'], 'diagram.exe', { type: 'application/octet-stream' }));

    await waitFor(() => expect(screen.getByText(/Unsupported file format/)).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

/** What the backend answers when it also read the WHOLE document into
 *  numbered requirements (`requirements_document`). */
function extractionWithRequirements(overrides: Record<string, unknown> = {}, requirementsDocument: Record<string, unknown> = {}) {
  return {
    text: 'x'.repeat(20000),
    char_count: 20000,
    truncated: true,
    requirements_document: {
      document_id: '11111111-1111-4111-8111-111111111111',
      full_char_count: 90000,
      full_truncated: false,
      sections_count: 12,
      requirements_count: 140,
      skipped_sections: ['Table of Contents', 'Revision History'],
      by_kind: { functional: 100, security: 40 },
      ...requirementsDocument,
    },
    ...overrides,
  };
}

describe('DocumentUploadSection — Phase 3, the whole document read into requirements', () => {
  it('sends the slot the file was dropped on, so its requirements are numbered from it', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, extractionWithRequirements()));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    await dropFile(new File(['content'], 'FRD.docx'));

    await waitFor(() => expect(screen.getByTestId('doc-requirements-prd')).toBeTruthy());
    const sent = JSON.parse(String((fetchMock.mock.calls[0] as [string, RequestInit])[1].body));
    expect(sent.document_type).toBe('PRD');
  });

  it('says how much was read in full and how many requirements it holds, not just the 20,000 characters analyzed', async () => {
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, extractionWithRequirements())));
    setApiKey(RAW_KEY);
    render(<Harness />);

    await dropFile(new File(['content'], 'FRD.docx'));

    const line = await screen.findByTestId('doc-requirements-prd');
    expect(line.textContent).toContain('Read in full: 90,000 chars · 140 requirements in 12 sections');
    expect(line.textContent).toContain('100 functional · 40 security');
    expect(line.textContent).toContain('Left out on purpose: Table of Contents, Revision History');
    // The old line still tells the truth about the pasted text.
    expect(screen.getByText(/20,000 chars of real content analyzed \(truncated\)/)).toBeTruthy();
    expect(line.textContent).toContain('Pasted-text generation still uses only the first 20,000 chars');
  });

  it('does not talk about the pasted-text limit for a document that fits in it', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async () =>
        jsonResponse(200, extractionWithRequirements({ text: 'short', char_count: 5, truncated: false }, { full_char_count: 5 })),
      ),
    );
    setApiKey(RAW_KEY);
    render(<Harness />);

    await dropFile(new File(['short'], 'FRD.md'));

    const line = await screen.findByTestId('doc-requirements-prd');
    expect(line.textContent).not.toContain('Pasted-text generation');
  });

  it('discloses the safety ceiling when even the whole-document read was cut', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async () =>
        jsonResponse(200, extractionWithRequirements({}, { full_char_count: 3_000_000, full_truncated: true })),
      ),
    );
    setApiKey(RAW_KEY);
    render(<Harness />);

    await dropFile(new File(['content'], 'HUGE.txt'));

    const line = await screen.findByTestId('doc-requirements-prd');
    expect(line.textContent).toContain('Only the first 3,000,000 characters were read');
  });

  it('shows no requirements line when the backend did not report one — the file is used as before', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async () => jsonResponse(200, { text: 'Real requirement content.', char_count: 26, truncated: false })),
    );
    setApiKey(RAW_KEY);
    render(<Harness />);

    await dropFile(new File(['Real requirement content.'], 'BRD.md'));

    await waitFor(() => expect(screen.getByText(/26 chars of real content analyzed/)).toBeTruthy());
    expect(screen.queryByTestId('doc-requirements-prd')).toBeNull();
    expect(screen.queryByText(/Read in full/)).toBeNull();
  });

  it('hands the document id and counts to the page, and a replacement file replaces them', async () => {
    const answers = [
      extractionWithRequirements(),
      extractionWithRequirements(
        { text: 'new', char_count: 3, truncated: false },
        { document_id: '22222222-2222-4222-8222-222222222222', requirements_count: 7, full_char_count: 3 },
      ),
    ];
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, answers.shift())));
    setApiKey(RAW_KEY);
    const seen: UploadedRequirementDoc[][] = [];
    function Spy() {
      const [docs, setDocs] = React.useState<UploadedRequirementDoc[]>([]);
      seen.push(docs);
      return <DocumentUploadSection documents={docs} onDocumentsChange={setDocs} projectId={PROJECT_ID} />;
    }
    render(<Spy />);

    await dropFile(new File(['one'], 'FRD.docx'));
    await screen.findByTestId('doc-requirements-prd');
    await dropFile(new File(['two'], 'FRD-v2.docx'));
    await waitFor(() => expect(screen.getByTestId('doc-requirements-prd').textContent).toContain('7 requirements'));

    const last = seen[seen.length - 1];
    expect(last).toHaveLength(1);
    expect(last[0]).toMatchObject({
      requirementsDocumentId: '22222222-2222-4222-8222-222222222222',
      requirementsCount: 7,
      fullCharCount: 3,
    });
  });

  it('now says PDF content is analyzed, and only the legacy .doc is not', () => {
    render(<DocumentUploadSection documents={[]} onDocumentsChange={() => {}} projectId={null} />);
    expect(screen.getByText(/Content analyzed: PDF, DOCX, MD, TXT, JSON, YAML\. Legacy \.doc is attached but not content-analyzed\./)).toBeTruthy();
  });
});

function noop() {}

describe('DocumentUploadSection — BRD slot', () => {
  it('renders all 5 slots, BRD first, with the real 5 SLOTS count', () => {
    const { container } = render(
      <DocumentUploadSection documents={[]} onDocumentsChange={noop} projectId={null} />
    );

    expect(screen.getByText('5 SLOTS')).toBeTruthy();
    expect(document.getElementById('doc-slot-brd')).toBeTruthy();
    expect(document.getElementById('doc-slot-prd')).toBeTruthy();
    expect(document.getElementById('doc-slot-frd')).toBeTruthy();
    expect(document.getElementById('doc-slot-srd')).toBeTruthy();
    expect(document.getElementById('doc-slot-additional')).toBeTruthy();

    expect(screen.getByText('1. BRD')).toBeTruthy();
    expect(screen.getByText('Business Requirements Document')).toBeTruthy();
    // Renumbered to make room for BRD as slot 1.
    expect(screen.getByText('2. PRD')).toBeTruthy();
    expect(screen.getByText('5. Additional Document')).toBeTruthy();

    // BRD genuinely leads the Source-of-Truth Context Flow banner (and
    // therefore the whole page), ahead of PRD -- not just present somewhere.
    const text = container.textContent ?? '';
    const brdIndex = text.indexOf('BRD');
    const prdIndex = text.indexOf('PRD');
    expect(brdIndex).toBeGreaterThanOrEqual(0);
    expect(prdIndex).toBeGreaterThan(brdIndex);
  });

  it('shows a real uploaded BRD document as Ready in its own slot, independent of the other slots', () => {
    const brdDoc: UploadedRequirementDoc = {
      slotId: 'BRD',
      label: '1. BRD',
      fullName: 'Business Requirements Document',
      fileName: 'business-case.pdf',
      fileSize: 2048,
      fileType: 'PDF',
      uploadedAt: new Date().toISOString(),
      status: 'READY',
    };

    render(<DocumentUploadSection documents={[brdDoc]} onDocumentsChange={noop} projectId={null} />);

    const brdSlot = document.getElementById('doc-slot-brd')!;
    expect(brdSlot.textContent).toContain('business-case.pdf');
    expect(brdSlot.textContent).toContain('Ready');

    // The other 4 slots remain open — this document only fills BRD's slot.
    const prdSlot = document.getElementById('doc-slot-prd')!;
    expect(prdSlot.textContent).toContain('Slot Open');
  });

  it('lets a real file drop trigger real BRD extraction the same way any other slot already can', async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      jsonResponse(200, { text: 'Business case content.', char_count: 23, truncated: false })
    );
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    const input = document.querySelector('#doc-slot-brd input[type=file]') as HTMLInputElement;
    await act(async () => {
      fireEvent.change(input, {
        target: { files: [new File(['Business case content.'], 'BRD.md', { type: 'text/markdown' })] },
      });
    });

    await waitFor(() => expect(screen.getByText(/23 chars of real content analyzed/)).toBeTruthy());
  });
});

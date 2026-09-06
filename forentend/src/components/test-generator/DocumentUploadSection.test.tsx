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

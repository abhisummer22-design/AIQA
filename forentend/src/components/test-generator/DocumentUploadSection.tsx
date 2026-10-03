import React, { useState, useRef } from 'react';
import {
  FileText,
  UploadCloud,
  CheckCircle2,
  Trash2,
  RefreshCw,
  AlertCircle,
  AlertTriangle,
  FileCode,
  FileSpreadsheet,
  Info,
  Layers,
  ArrowDown,
  FileCheck,
  ListChecks,
} from 'lucide-react';
import { RequirementDocType, UploadedRequirementDoc } from '../../types';
import { ApiError, aiqaApi } from '../../services/aiqaApi';

interface DocumentUploadSectionProps {
  documents: UploadedRequirementDoc[];
  onDocumentsChange: (docs: UploadedRequirementDoc[]) => void;
  /** The real, currently-selected project id — required to call the real
   *  content-extraction endpoint (project-scoped, same as every other
   *  real-outbound-action call in this app). `null` means a file can still
   *  be attached, but its content cannot be analyzed yet. */
  projectId: string | null;
  disabled?: boolean;
}

/** Reads the file's real bytes as base64, via `FileReader.readAsDataURL`
 *  (which already produces base64) minus its `data:<mime>;base64,` prefix.
 *  `FileReader` rather than the newer `Blob.arrayBuffer()` deliberately:
 *  it is universally available (including in jsdom, where `arrayBuffer` is
 *  not), and it avoids hand-rolling a chunked byte->binary-string loop
 *  that would otherwise be needed to dodge `String.fromCharCode`'s
 *  argument-count limit on a large file. */
function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error('could not read this file'));
    reader.onload = () => {
      const result = String(reader.result ?? '');
      const comma = result.indexOf(',');
      resolve(comma >= 0 ? result.slice(comma + 1) : '');
    };
    reader.readAsDataURL(file);
  });
}

interface DocSlotConfig {
  id: RequirementDocType;
  label: string;
  fullName: string;
  description: string;
  roleInHierarchy: string;
  badgeColor: string;
}

const DOCUMENT_SLOTS: DocSlotConfig[] = [
  {
    id: 'BRD',
    label: '1. BRD',
    fullName: 'Business Requirements Document',
    description: 'Business goals, target market, ROI justification, and stakeholder success criteria.',
    roleInHierarchy: 'Top-level Business Case & Goals',
    badgeColor: 'border-fuchsia-700/60 bg-fuchsia-950/40 text-fuchsia-300',
  },
  {
    id: 'PRD',
    label: '2. PRD',
    fullName: 'Product Requirements Document',
    description: 'Core product vision, user personas, success metrics, and scope boundaries.',
    roleInHierarchy: 'Product Vision, Derived from BRD',
    badgeColor: 'border-yellow-700/60 bg-yellow-950/40 text-yellow-300',
  },
  {
    id: 'FRD',
    label: '3. FRD',
    fullName: 'Functional Requirements Document',
    description: 'Detailed feature behavior, user stories, interaction flows, and acceptance criteria.',
    roleInHierarchy: 'Functional Mechanics & Rules',
    badgeColor: 'border-violet-700/60 bg-violet-950/40 text-violet-300',
  },
  {
    id: 'SRD',
    label: '4. SRD',
    fullName: 'System Requirements Document',
    description: 'Architecture specifications, protocol constraints, SLA benchmarks, and API contracts.',
    roleInHierarchy: 'System Architecture & SLAs',
    badgeColor: 'border-purple-700/60 bg-purple-950/40 text-purple-300',
  },
  {
    id: 'ADDITIONAL',
    label: '5. Additional Document',
    fullName: 'Supporting Specification / Reference',
    description: 'Database schemas, OpenAPI definitions, edge-case logs, or legacy runbooks.',
    roleInHierarchy: 'Supplemental Technical Context',
    badgeColor: 'border-amber-700/60 bg-amber-950/40 text-amber-300',
  },
];

const ACCEPTED_EXTENSIONS = ['.pdf', '.docx', '.doc', '.md', '.txt', '.json', '.yaml', '.yml'];

/** The kinds a requirement can have, in the order they are listed. */
const REQUIREMENT_KIND_ORDER = ['functional', 'security', 'performance', 'usability', 'interface', 'data', 'context', 'reference'];

/** "100 functional · 40 security", biggest groups in a fixed, readable order;
 *  a kind this UI does not know yet is still listed, after the known ones. */
function describeKinds(byKind: Record<string, number>): string {
  const known = REQUIREMENT_KIND_ORDER.filter((k) => (byKind[k] ?? 0) > 0);
  const unknown = Object.keys(byKind).filter((k) => !REQUIREMENT_KIND_ORDER.includes(k) && byKind[k] > 0);
  return [...known, ...unknown].map((k) => `${byKind[k].toLocaleString()} ${k}`).join(' · ');
}

/** The sections left out on purpose, without letting a long list swamp the card. */
function describeSkipped(sections: string[]): string {
  const shown = sections.slice(0, 4).join(', ');
  return sections.length > 4 ? `${shown} +${sections.length - 4} more` : shown;
}

export const DocumentUploadSection: React.FC<DocumentUploadSectionProps> = ({
  documents,
  onDocumentsChange,
  projectId,
  disabled = false,
}) => {
  const [dragOverSlot, setDragOverSlot] = useState<RequirementDocType | null>(null);
  const [uploadProgress, setUploadProgress] = useState<Record<RequirementDocType, number>>({
    BRD: 0,
    PRD: 0,
    FRD: 0,
    SRD: 0,
    ADDITIONAL: 0,
  });
  const [errors, setErrors] = useState<Record<RequirementDocType, string | null>>({
    BRD: null,
    PRD: null,
    FRD: null,
    SRD: null,
    ADDITIONAL: null,
  });

  const fileInputRefs = useRef<Record<RequirementDocType, HTMLInputElement | null>>({
    BRD: null,
    PRD: null,
    FRD: null,
    SRD: null,
    ADDITIONAL: null,
  });

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  };

  const validateFile = (file: File): string | null => {
    const ext = `.${file.name.split('.').pop()?.toLowerCase()}`;
    if (!ACCEPTED_EXTENSIONS.includes(ext)) {
      return `Unsupported file format (${ext}). Supported: PDF, DOCX, MD, TXT, JSON, YAML`;
    }
    const maxSizeBytes = 40 * 1024 * 1024; // 40MB
    if (file.size > maxSizeBytes) {
      return 'File size exceeds 40MB limit.';
    }
    return null;
  };

  const handleFileUpload = async (slotId: RequirementDocType, file: File) => {
    const error = validateFile(file);
    if (error) {
      setErrors((prev) => ({ ...prev, [slotId]: error }));
      return;
    }

    setErrors((prev) => ({ ...prev, [slotId]: null }));
    // A real in-flight indicator tied to the real request below — not a
    // fixed-duration fake progress ramp with no actual work behind it.
    setUploadProgress((prev) => ({ ...prev, [slotId]: 1 }));

    const slotConfig = DOCUMENT_SLOTS.find((s) => s.id === slotId)!;
    const baseDoc: UploadedRequirementDoc = {
      slotId,
      label: slotConfig.label,
      fullName: slotConfig.fullName,
      fileName: file.name,
      fileSize: file.size,
      fileType: file.name.split('.').pop()?.toUpperCase() || 'DOCUMENT',
      uploadedAt: new Date().toISOString(),
      status: 'READY',
    };

    const commit = (doc: UploadedRequirementDoc) => {
      setUploadProgress((prev) => ({ ...prev, [slotId]: 0 }));
      const existingFiltered = documents.filter((d) => d.slotId !== slotId);
      onDocumentsChange([...existingFiltered, doc]);
    };

    if (!projectId) {
      // The file is genuinely attached (real name/size/type below), it
      // just cannot be sent for real content extraction yet — a project
      // is required since the endpoint is project-scoped, same rule as
      // Application Source verification.
      commit({ ...baseDoc, extractionError: 'Select a project before its real content can be analyzed.' });
      return;
    }

    try {
      const contentBase64 = await fileToBase64(file);
      const result = await aiqaApi.documents.extractText(projectId, {
        file_name: file.name,
        content_base64: contentBase64,
        document_type: slotId,
      });
      // `requirements_document` is present only when the backend also read
      // the WHOLE document and stored its numbered requirements — an older
      // backend, or one that could not store it, simply omits it and the
      // file is used exactly as before.
      const read = result.requirements_document;
      commit({
        ...baseDoc,
        extractedText: result.text,
        extractedCharCount: result.char_count,
        extractedTruncated: result.truncated,
        ...(read
          ? {
              requirementsDocumentId: read.document_id,
              fullCharCount: read.full_char_count,
              fullTruncated: read.full_truncated,
              requirementsCount: read.requirements_count,
              sectionsCount: read.sections_count,
              skippedSections: read.skipped_sections,
              requirementsByKind: read.by_kind as Record<string, number>,
            }
          : {}),
      });
    } catch (err) {
      // A real extraction failure (e.g. a legacy .doc/.pdf this app
      // cannot parse, or a genuine network/backend error) is disclosed
      // honestly, never silently treated as if the content was read.
      commit({
        ...baseDoc,
        extractionError: err instanceof ApiError ? err.message : 'Could not analyze this document\'s real content.',
      });
    }
  };

  const handleRemoveDoc = (slotId: RequirementDocType) => {
    onDocumentsChange(documents.filter((d) => d.slotId !== slotId));
    setErrors((prev) => ({ ...prev, [slotId]: null }));
    setUploadProgress((prev) => ({ ...prev, [slotId]: 0 }));
  };

  const handleDragOver = (e: React.DragEvent, slotId: RequirementDocType) => {
    e.preventDefault();
    if (disabled) return;
    setDragOverSlot(slotId);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setDragOverSlot(null);
  };

  const handleDrop = (e: React.DragEvent, slotId: RequirementDocType) => {
    e.preventDefault();
    setDragOverSlot(null);
    if (disabled) return;
    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      handleFileUpload(slotId, e.dataTransfer.files[0]);
    }
  };

  const triggerBrowse = (slotId: RequirementDocType) => {
    if (disabled) return;
    fileInputRefs.current[slotId]?.click();
  };

  return (
    <div id="product-documentation-section" className="space-y-4">
      {/* Section Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
        <div>
          <div className="flex items-center gap-2">
            <Layers className="w-4 h-4 text-yellow-400" />
            <h2 className="text-sm font-bold text-slate-100 font-sans tracking-tight">
              Product Documentation
            </h2>
            <span className="text-[10px] font-mono font-semibold px-2 py-0.5 rounded bg-yellow-950/60 border border-yellow-800/60 text-yellow-300">
              5 SLOTS
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans mt-0.5">
            Upload the documents AIQA should use as the source of truth for test generation.
          </p>
        </div>

        {/* Accepted Formats Tag */}
        <div className="flex items-center gap-1.5 text-[10px] font-mono text-slate-400 bg-[#020617] px-2.5 py-1 rounded border border-slate-800">
          <FileCode className="w-3 h-3 text-violet-400" />
          <span>Content analyzed: PDF, DOCX, MD, TXT, JSON, YAML. Legacy .doc is attached but not content-analyzed.</span>
        </div>
      </div>

      {/* Visual Source-of-Truth Hierarchy Banner */}
      <div className="p-3 rounded-lg bg-gradient-to-r from-[#020617] via-[#0B1120] to-[#020617] border border-violet-900/40 text-xs">
        <div className="flex items-center gap-2 text-violet-300 font-mono font-semibold text-[11px] mb-2">
          <Info className="w-3.5 h-3.5 text-violet-400" />
          <span>Source-of-Truth Context Flow</span>
        </div>

        {/* Horizontal Hierarchy Diagram */}
        <div className="flex flex-wrap items-center gap-1.5 text-[10px] font-mono">
          <span className="px-2 py-0.5 rounded bg-fuchsia-950/80 border border-fuchsia-800 text-fuchsia-300 font-bold">
            BRD
          </span>
          <ArrowDown className="w-3 h-3 text-content-muted rotate-[-90deg] shrink-0" />
          <span className="px-2 py-0.5 rounded bg-yellow-950/80 border border-yellow-800 text-yellow-300 font-bold">
            PRD
          </span>
          <ArrowDown className="w-3 h-3 text-content-muted rotate-[-90deg] shrink-0" />
          <span className="px-2 py-0.5 rounded bg-violet-950/80 border border-violet-800 text-violet-300 font-bold">
            FRD
          </span>
          <ArrowDown className="w-3 h-3 text-content-muted rotate-[-90deg] shrink-0" />
          <span className="px-2 py-0.5 rounded bg-purple-950/80 border border-purple-800 text-purple-300 font-bold">
            SRD
          </span>
          <ArrowDown className="w-3 h-3 text-content-muted rotate-[-90deg] shrink-0" />
          <span className="px-2 py-0.5 rounded bg-amber-950/80 border border-amber-800 text-amber-300 font-bold">
            Additional Context
          </span>
        </div>

        <div className="text-[10px] text-slate-400 font-sans mt-2">
          AIQA will use uploaded documentation as generation context.
        </div>
      </div>

      {/* 4 Slots Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
        {DOCUMENT_SLOTS.map((slot) => {
          const uploadedDoc = documents.find((d) => d.slotId === slot.id);
          const isUploading = (uploadProgress[slot.id] || 0) > 0;
          const errorMsg = errors[slot.id];
          const isDragOver = dragOverSlot === slot.id;

          return (
            <div
              key={slot.id}
              id={`doc-slot-${slot.id.toLowerCase()}`}
              onDragOver={(e) => handleDragOver(e, slot.id)}
              onDragLeave={handleDragLeave}
              onDrop={(e) => handleDrop(e, slot.id)}
              className={`relative rounded-lg p-3.5 flex flex-col justify-between transition-all duration-200 border ${
                uploadedDoc
                  ? 'bg-[#0B1120] border-emerald-900/60 shadow-sm'
                  : isDragOver
                  ? 'bg-violet-950/30 border-violet-500 border-dashed'
                  : 'bg-[#0B1120] border-slate-800/90 hover:border-slate-700'
              }`}
            >
              {/* Hidden File Input */}
              <input
                ref={(el) => (fileInputRefs.current[slot.id] = el)}
                type="file"
                aria-label={`Upload ${slot.label}`}
                accept=".pdf,.docx,.doc,.md,.txt,.json,.yaml,.yml"
                className="hidden"
                onChange={(e) => {
                  if (e.target.files && e.target.files.length > 0) {
                    handleFileUpload(slot.id, e.target.files[0]);
                  }
                }}
              />

              {/* Slot Header */}
              <div className="space-y-1">
                <div className="flex items-center justify-between">
                  <span
                    className={`inline-flex items-center px-2 py-0.5 rounded text-[10px] font-mono font-bold border ${slot.badgeColor}`}
                  >
                    {slot.label}
                  </span>

                  {uploadedDoc ? (
                    <span className="inline-flex items-center gap-1 text-[10px] font-mono font-semibold text-emerald-400">
                      <CheckCircle2 className="w-3 h-3" />
                      <span>Ready</span>
                    </span>
                  ) : (
                    <span className="text-[10px] font-mono text-content-muted">Slot Open</span>
                  )}
                </div>

                <div className="font-bold text-xs text-slate-200 font-sans mt-1">
                  {slot.fullName}
                </div>
                <div className="text-[11px] text-slate-400 font-sans leading-tight">
                  {slot.description}
                </div>
              </div>

              {/* Slot Content Body */}
              <div className="mt-3 pt-2.5 border-t border-slate-800/80">
                {uploadedDoc ? (
                  /* Uploaded State */
                  <div className="space-y-2">
                    <div className="flex items-start gap-2 p-2 rounded bg-[#020617] border border-slate-800">
                      <FileCheck className="w-4 h-4 text-emerald-400 shrink-0 mt-0.5" />
                      <div className="min-w-0 flex-1">
                        <div className="text-xs font-mono font-bold text-slate-100 truncate">
                          {uploadedDoc.fileName}
                        </div>
                        <div className="flex items-center gap-2 text-[10px] font-mono text-slate-400 mt-0.5">
                          <span>{formatFileSize(uploadedDoc.fileSize)}</span>
                          <span>•</span>
                          <span className="text-emerald-400">{uploadedDoc.fileType}</span>
                        </div>
                      </div>
                    </div>

                    {/* Real content-analysis status — never claims content
                        was read when it wasn't, and never hides a real
                        extraction failure. */}
                    {uploadedDoc.extractedText != null ? (
                      <div className="flex items-center gap-1.5 px-2 py-1 rounded bg-emerald-950/30 border border-emerald-900/60 text-[10px] font-mono text-emerald-300">
                        <CheckCircle2 className="w-3 h-3 shrink-0" />
                        <span>
                          {uploadedDoc.extractedCharCount?.toLocaleString()} chars of real content analyzed
                          {uploadedDoc.extractedTruncated ? ' (truncated)' : ''}
                        </span>
                      </div>
                    ) : uploadedDoc.extractionError ? (
                      <div className="flex items-start gap-1.5 px-2 py-1 rounded bg-amber-950/30 border border-amber-900/60 text-[10px] font-mono text-amber-300">
                        <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
                        <span>Content not analyzed: {uploadedDoc.extractionError}</span>
                      </div>
                    ) : null}

                    {/* What reading the WHOLE document found — the text
                        analyzed above stops at 20,000 characters, this does
                        not. Shown only when the backend really stored it. */}
                    {uploadedDoc.requirementsDocumentId ? (
                      <div
                        data-testid={`doc-requirements-${slot.id.toLowerCase()}`}
                        className="flex items-start gap-1.5 px-2 py-1 rounded bg-violet-950/30 border border-violet-900/60 text-[10px] font-mono text-violet-200"
                      >
                        <ListChecks className="w-3 h-3 shrink-0 mt-0.5 text-violet-400" />
                        <div className="space-y-0.5 min-w-0">
                          <div>
                            Read in full: {(uploadedDoc.fullCharCount ?? 0).toLocaleString()} chars ·{' '}
                            {(uploadedDoc.requirementsCount ?? 0).toLocaleString()} requirements in{' '}
                            {(uploadedDoc.sectionsCount ?? 0).toLocaleString()} sections
                          </div>
                          {uploadedDoc.requirementsByKind && Object.keys(uploadedDoc.requirementsByKind).length > 0 ? (
                            <div className="text-violet-300/80">{describeKinds(uploadedDoc.requirementsByKind)}</div>
                          ) : null}
                          {uploadedDoc.skippedSections && uploadedDoc.skippedSections.length > 0 ? (
                            <div className="text-slate-400">Left out on purpose: {describeSkipped(uploadedDoc.skippedSections)}</div>
                          ) : null}
                          {uploadedDoc.fullTruncated ? (
                            <div className="text-amber-300">
                              Only the first 3,000,000 characters were read (a safety limit).
                            </div>
                          ) : null}
                          {uploadedDoc.extractedTruncated ? (
                            <div className="text-slate-400">
                              Pasted-text generation still uses only the first 20,000 chars; "Generate from Numbered Requirements" uses all of it.
                            </div>
                          ) : null}
                        </div>
                      </div>
                    ) : null}

                    {/* Actions: Replace / Remove */}
                    <div className="flex items-center justify-between pt-1">
                      <button
                        type="button"
                        onClick={() => triggerBrowse(slot.id)}
                        disabled={disabled}
                        className="inline-flex items-center gap-1 text-[11px] font-mono text-violet-400 hover:text-violet-300 font-medium cursor-pointer transition-colors disabled:opacity-50"
                      >
                        <RefreshCw className="w-3 h-3" />
                        <span>Replace File</span>
                      </button>

                      <button
                        type="button"
                        onClick={() => handleRemoveDoc(slot.id)}
                        disabled={disabled}
                        className="inline-flex items-center gap-1 text-[11px] font-mono text-rose-400 hover:text-rose-300 font-medium cursor-pointer transition-colors disabled:opacity-50"
                      >
                        <Trash2 className="w-3 h-3" />
                        <span>Remove</span>
                      </button>
                    </div>
                  </div>
                ) : isUploading ? (
                  /* Real extraction in flight — an indeterminate indicator,
                     since a single real request has no meaningful
                     percentage-complete signal to report (unlike the
                     fixed-duration fake ramp this replaced). */
                  <div className="p-3 rounded bg-[#020617] border border-violet-900/60">
                    <div className="flex items-center gap-1.5 text-xs font-mono text-violet-300">
                      <RefreshCw className="w-3 h-3 animate-spin text-violet-400" />
                      <span>Analyzing real document content...</span>
                    </div>
                  </div>
                ) : (
                  /* Empty Dropzone State */
                  // A <button>, because this was a <div onClick> with no
                  // keyboard path at all: a keyboard user could not open the file
                  // picker for any slot, so no document could be attached
                  // without a mouse. The word "browse" was a plain <span>, so it
                  // was not a control either -- it only looked like one.
                  // Click-to-browse and drag-and-drop both still work; the
                  // native file input this triggers is unchanged.
                  <button
                    type="button"
                    onClick={() => triggerBrowse(slot.id)}
                    aria-label={`Browse to upload a ${slot.label} document`}
                    className="p-3 rounded border border-dashed border-slate-700/80 hover:border-violet-500/80 bg-[#020617]/50 hover:bg-[#020617] transition-all cursor-pointer text-center flex flex-col items-center justify-center space-y-1.5 w-full focus-visible:ring-2 focus-visible:ring-violet-400 focus-visible:outline-none"
                  >
                    <UploadCloud className="w-5 h-5 text-content-muted group-hover:text-violet-400" />
                    <div className="text-[11px] font-mono font-semibold text-content-secondary">
                      Drag & drop or <span className="text-violet-400 underline">browse</span>
                    </div>
                    <div className="text-[10px] font-mono text-content-muted">
                      No document uploaded
                    </div>
                  </button>
                )}

                {/* Error Banner */}
                {errorMsg && (
                  <div className="mt-2 p-2 rounded bg-rose-950/40 border border-rose-800/80 text-[10px] font-mono text-rose-300 flex items-center gap-1.5">
                    <AlertCircle className="w-3 h-3 text-rose-400 shrink-0" />
                    <span>{errorMsg}</span>
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

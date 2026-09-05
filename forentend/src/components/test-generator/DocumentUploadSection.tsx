import React, { useState, useRef } from 'react';
import {
  FileText,
  UploadCloud,
  CheckCircle2,
  Trash2,
  RefreshCw,
  AlertCircle,
  FileCode,
  FileSpreadsheet,
  Info,
  Layers,
  ArrowDown,
  FileCheck,
} from 'lucide-react';
import { RequirementDocType, UploadedRequirementDoc } from '../../types';

interface DocumentUploadSectionProps {
  documents: UploadedRequirementDoc[];
  onDocumentsChange: (docs: UploadedRequirementDoc[]) => void;
  disabled?: boolean;
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
    badgeColor: 'border-cyan-700/60 bg-cyan-950/40 text-cyan-300',
  },
  {
    id: 'FRD',
    label: '3. FRD',
    fullName: 'Functional Requirements Document',
    description: 'Detailed feature behavior, user stories, interaction flows, and acceptance criteria.',
    roleInHierarchy: 'Functional Mechanics & Rules',
    badgeColor: 'border-indigo-700/60 bg-indigo-950/40 text-indigo-300',
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

export const DocumentUploadSection: React.FC<DocumentUploadSectionProps> = ({
  documents,
  onDocumentsChange,
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

  const handleFileUpload = (slotId: RequirementDocType, file: File) => {
    const error = validateFile(file);
    if (error) {
      setErrors((prev) => ({ ...prev, [slotId]: error }));
      return;
    }

    setErrors((prev) => ({ ...prev, [slotId]: null }));
    setUploadProgress((prev) => ({ ...prev, [slotId]: 10 }));

    const slotConfig = DOCUMENT_SLOTS.find((s) => s.id === slotId)!;

    // Simulate real asynchronous file ingestion
    let currentPct = 15;
    const interval = setInterval(() => {
      currentPct += 25;
      if (currentPct >= 100) {
        clearInterval(interval);
        setUploadProgress((prev) => ({ ...prev, [slotId]: 0 }));

        const newDoc: UploadedRequirementDoc = {
          slotId,
          label: slotConfig.label,
          fullName: slotConfig.fullName,
          fileName: file.name,
          fileSize: file.size,
          fileType: file.name.split('.').pop()?.toUpperCase() || 'DOCUMENT',
          uploadedAt: new Date().toISOString(),
          status: 'READY',
        };

        const existingFiltered = documents.filter((d) => d.slotId !== slotId);
        onDocumentsChange([...existingFiltered, newDoc]);
      } else {
        setUploadProgress((prev) => ({ ...prev, [slotId]: currentPct }));
      }
    }, 120);
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
            <Layers className="w-4 h-4 text-cyan-400" />
            <h2 className="text-sm font-bold text-slate-100 font-sans tracking-tight">
              Product Documentation
            </h2>
            <span className="text-[10px] font-mono font-semibold px-2 py-0.5 rounded bg-cyan-950/60 border border-cyan-800/60 text-cyan-300">
              5 SLOTS
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans mt-0.5">
            Upload the documents AIQA should use as the source of truth for test generation.
          </p>
        </div>

        {/* Accepted Formats Tag */}
        <div className="flex items-center gap-1.5 text-[10px] font-mono text-slate-400 bg-[#020617] px-2.5 py-1 rounded border border-slate-800">
          <FileCode className="w-3 h-3 text-indigo-400" />
          <span>Accepted: PDF, DOCX, MD, TXT, JSON, YAML</span>
        </div>
      </div>

      {/* Visual Source-of-Truth Hierarchy Banner */}
      <div className="p-3 rounded-lg bg-gradient-to-r from-[#020617] via-[#0B1120] to-[#020617] border border-indigo-900/40 text-xs">
        <div className="flex items-center gap-2 text-indigo-300 font-mono font-semibold text-[11px] mb-2">
          <Info className="w-3.5 h-3.5 text-indigo-400" />
          <span>Source-of-Truth Context Flow</span>
        </div>

        {/* Horizontal Hierarchy Diagram */}
        <div className="flex flex-wrap items-center gap-1.5 text-[10px] font-mono">
          <span className="px-2 py-0.5 rounded bg-fuchsia-950/80 border border-fuchsia-800 text-fuchsia-300 font-bold">
            BRD
          </span>
          <ArrowDown className="w-3 h-3 text-slate-500 rotate-[-90deg] shrink-0" />
          <span className="px-2 py-0.5 rounded bg-cyan-950/80 border border-cyan-800 text-cyan-300 font-bold">
            PRD
          </span>
          <ArrowDown className="w-3 h-3 text-slate-500 rotate-[-90deg] shrink-0" />
          <span className="px-2 py-0.5 rounded bg-indigo-950/80 border border-indigo-800 text-indigo-300 font-bold">
            FRD
          </span>
          <ArrowDown className="w-3 h-3 text-slate-500 rotate-[-90deg] shrink-0" />
          <span className="px-2 py-0.5 rounded bg-purple-950/80 border border-purple-800 text-purple-300 font-bold">
            SRD
          </span>
          <ArrowDown className="w-3 h-3 text-slate-500 rotate-[-90deg] shrink-0" />
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
                  ? 'bg-indigo-950/30 border-indigo-500 border-dashed'
                  : 'bg-[#0B1120] border-slate-800/90 hover:border-slate-700'
              }`}
            >
              {/* Hidden File Input */}
              <input
                ref={(el) => (fileInputRefs.current[slot.id] = el)}
                type="file"
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
                    <span className="text-[10px] font-mono text-slate-500">Slot Open</span>
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

                    {/* Actions: Replace / Remove */}
                    <div className="flex items-center justify-between pt-1">
                      <button
                        type="button"
                        onClick={() => triggerBrowse(slot.id)}
                        disabled={disabled}
                        className="inline-flex items-center gap-1 text-[11px] font-mono text-indigo-400 hover:text-indigo-300 font-medium cursor-pointer transition-colors disabled:opacity-50"
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
                  /* Uploading Progress State */
                  <div className="p-3 rounded bg-[#020617] border border-indigo-900/60 space-y-2">
                    <div className="flex items-center justify-between text-xs font-mono text-indigo-300">
                      <span className="flex items-center gap-1.5">
                        <RefreshCw className="w-3 h-3 animate-spin text-indigo-400" />
                        <span>Reading & Ingesting File...</span>
                      </span>
                      <span>{uploadProgress[slot.id]}%</span>
                    </div>
                    <div className="h-1.5 w-full bg-slate-800 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-indigo-500 rounded-full transition-all duration-200"
                        style={{ width: `${uploadProgress[slot.id]}%` }}
                      />
                    </div>
                  </div>
                ) : (
                  /* Empty Dropzone State */
                  <div
                    onClick={() => triggerBrowse(slot.id)}
                    className="p-3 rounded border border-dashed border-slate-700/80 hover:border-indigo-500/80 bg-[#020617]/50 hover:bg-[#020617] transition-all cursor-pointer text-center flex flex-col items-center justify-center space-y-1.5"
                  >
                    <UploadCloud className="w-5 h-5 text-slate-500 group-hover:text-indigo-400" />
                    <div className="text-[11px] font-mono font-semibold text-slate-300">
                      Drag & drop or <span className="text-indigo-400 underline">browse</span>
                    </div>
                    <div className="text-[10px] font-mono text-slate-400">
                      No document uploaded
                    </div>
                  </div>
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

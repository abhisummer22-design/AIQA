/**
 * Evidence viewer — fetches the real artifact bytes.
 *
 * `GET /evidence/:id/content` streams whatever was captured at execution
 * time, with the content type recorded then. Text-ish artifacts (logs,
 * traces, JSON) are rendered inline; anything else is offered as a download
 * rather than guessed at. The previous UI shipped three hand-written
 * "evidence" blobs — an invented PostgreSQL deadlock trace among them —
 * embedded in the frontend bundle.
 */

import React from "react";
import { Download, X } from "lucide-react";
import { useApiResource } from "../../hooks/useApi";
import { getEvidenceContent } from "../../services/orchestrator";
import type { EvidenceRow } from "../../types/orchestrator";
import { ResourceView } from "./StateViews";
import { Timestamp } from "./Badges";

interface LoadedEvidence {
  text: string | null;
  blob: Blob;
}

function isTextLike(contentType: string | null): boolean {
  if (!contentType) return true; // orchestrator defaults to text/plain
  return (
    contentType.startsWith("text/") ||
    contentType.includes("json") ||
    contentType.includes("xml") ||
    contentType.includes("yaml")
  );
}

export function EvidenceViewer({ evidence, onClose }: { evidence: EvidenceRow; onClose: () => void }) {
  const content = useApiResource<LoadedEvidence>(
    async (signal) => {
      const blob = await getEvidenceContent(evidence.id, signal);
      const text = isTextLike(evidence.content_type) ? await blob.text() : null;
      return { text, blob };
    },
    [evidence.id],
    { isEmpty: () => false },
  );

  const download = () => {
    if (!content.data) return;
    const url = URL.createObjectURL(content.data.blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = evidence.object_key.split("/").pop() || `evidence-${evidence.id}`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4">
      <div className="flex max-h-[85vh] w-full max-w-4xl flex-col rounded-xl border border-slate-700 bg-[#0a0f1a] shadow-2xl">
        <div className="flex items-start justify-between border-b border-slate-800 px-4 py-3">
          <div className="min-w-0">
            <h2 className="truncate text-sm font-semibold text-slate-100">{evidence.object_key}</h2>
            <div className="mt-0.5 font-mono text-[10px] text-slate-600">
              {evidence.kind} · {evidence.content_type ?? "text/plain"} · {evidence.size_bytes} bytes ·{" "}
              <Timestamp value={evidence.created_at} />
            </div>
          </div>
          <div className="ml-3 flex shrink-0 items-center gap-1.5">
            <button
              onClick={download}
              disabled={!content.data}
              title="Download artifact"
              className="rounded border border-slate-700 p-1.5 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200 disabled:opacity-30"
            >
              <Download size={13} />
            </button>
            <button
              onClick={onClose}
              className="rounded border border-slate-700 p-1.5 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
            >
              <X size={13} />
            </button>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          <ResourceView state={content} loadingLabel="Fetching artifact…">
            {(loaded) =>
              loaded.text !== null ? (
                <pre className="whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-slate-300">
                  {loaded.text}
                </pre>
              ) : (
                <div className="py-8 text-center text-xs text-slate-500">
                  Binary artifact ({evidence.content_type}). Use Download to retrieve it.
                </div>
              )
            }
          </ResourceView>
        </div>
      </div>
    </div>
  );
}

import React, { useEffect, useState } from 'react';
import { AlertTriangle, Loader2, Trash2, X } from 'lucide-react';
import { ApiError } from '../../services/aiqaApi';
import type { Project } from '../../types/contract';

export interface DeleteProjectDialogProps {
  isOpen: boolean;
  project: Project | null;
  isActiveProject: boolean;
  onClose: () => void;
  onConfirmDelete: (projectId: string) => Promise<void>;
  onSuccess?: (projectName: string) => void;
}

export const DeleteProjectDialog: React.FC<DeleteProjectDialogProps> = ({
  isOpen,
  project,
  isActiveProject,
  onClose,
  onConfirmDelete,
  onSuccess,
}) => {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      setError(null);
      setSubmitting(false);
    }
  }, [isOpen, project]);

  useEffect(() => {
    if (!isOpen) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !submitting) {
        onClose();
      }
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [isOpen, submitting, onClose]);

  if (!isOpen || !project) return null;

  async function handleDelete(): Promise<void> {
    if (!project || submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirmDelete(project.id);
      onSuccess?.(project.name);
      onClose();
    } catch (err) {
      const message =
        err instanceof ApiError
          ? err.message
          : 'Failed to delete project. The project was not removed. Please try again.';
      setError(message);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-16 bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100"
      onClick={() => !submitting && onClose()}
    >
      <div
        id="delete-project-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Delete Project"
        className="w-full max-w-md rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-slate-800 bg-[#020617]">
          <div className="flex items-center gap-2">
            <Trash2 className="w-4 h-4 text-rose-400" />
            <h2 className="text-sm font-bold text-slate-100 font-sans">
              {isActiveProject ? 'Delete Active Project?' : 'Delete Project?'}
            </h2>
          </div>
          <button
            type="button"
            onClick={onClose}
            disabled={submitting}
            aria-label="Close Delete Project dialog"
            className="p-1.5 text-content-muted hover:text-slate-300 rounded cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-4 space-y-3">
          {isActiveProject && (
            <div className="p-2.5 rounded-md bg-amber-950/40 border border-amber-800/60 flex items-start gap-2">
              <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
              <p className="text-xs text-amber-200">
                This project is currently <span className="font-semibold text-amber-100">active</span>. Deleting it will
                switch the dashboard to another accessible project or the empty state.
              </p>
            </div>
          )}

          <p className="text-xs text-slate-300 leading-relaxed">
            You are about to permanently delete <span className="font-semibold text-slate-100">&quot;{project.name}&quot;</span>.
          </p>

          <p className="text-xs text-slate-400 leading-relaxed">
            This action may remove associated project data including test cases, execution history, reports, findings,
            and other project-scoped records according to backend deletion rules.
          </p>

          <p className="text-[11px] font-mono text-rose-400/90 font-semibold">
            This action cannot be undone.
          </p>

          {error && (
            <div
              id="delete-project-error"
              className="p-2.5 rounded-md bg-rose-950/40 border border-rose-900/60 text-rose-300 text-xs flex items-start gap-2"
            >
              <AlertTriangle className="w-4 h-4 text-rose-400 shrink-0 mt-0.5" />
              <span>{error}</span>
            </div>
          )}
        </div>

        <div className="px-4 py-3 border-t border-slate-800 bg-[#020617] flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={submitting}
            className="px-3 py-1.5 rounded-md text-xs text-slate-400 hover:text-slate-200 cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed"
          >
            Cancel
          </button>
          <button
            type="button"
            id="confirm-delete-project-button"
            onClick={() => void handleDelete()}
            disabled={submitting}
            className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-rose-600 hover:bg-rose-500 text-white text-xs font-semibold transition-colors cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed"
          >
            {submitting ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                <span>Deleting…</span>
              </>
            ) : (
              <>
                <Trash2 className="w-3.5 h-3.5" />
                <span>Delete Project</span>
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );
};

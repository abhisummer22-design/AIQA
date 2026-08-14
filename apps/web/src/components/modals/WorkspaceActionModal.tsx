import React from "react";
import { AlertCircle, Check, X, ShieldAlert, Mail, FileSpreadsheet, Calendar } from "lucide-react";

export interface WorkspaceActionConfig {
  type: "gmail" | "sheets" | "calendar";
  actionTitle: string;
  actionDescription: string;
  details: Array<{ label: string; value: string }>;
  confirmButtonText: string;
  isDestructive?: boolean;
  onConfirm: () => Promise<void> | void;
}

interface WorkspaceActionModalProps {
  isOpen: boolean;
  onClose: () => void;
  config: WorkspaceActionConfig | null;
  isLoading?: boolean;
}

export const WorkspaceActionModal: React.FC<WorkspaceActionModalProps> = ({
  isOpen,
  onClose,
  config,
  isLoading = false
}) => {
  if (!isOpen || !config) return null;

  const getIcon = () => {
    switch (config.type) {
      case "gmail":
        return <Mail className="w-5 h-5 text-rose-400" />;
      case "sheets":
        return <FileSpreadsheet className="w-5 h-5 text-emerald-400" />;
      case "calendar":
        return <Calendar className="w-5 h-5 text-blue-400" />;
      default:
        return <AlertCircle className="w-5 h-5 text-indigo-400" />;
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm animate-in fade-in duration-150">
      <div
        className="bg-slate-900 border border-slate-700 rounded-xl shadow-2xl max-w-lg w-full overflow-hidden text-slate-200"
        role="dialog"
        aria-modal="true"
      >
        {/* Header */}
        <div className="p-5 border-b border-slate-800 bg-slate-950/60 flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="p-2.5 rounded-lg bg-slate-800 border border-slate-700">
              {getIcon()}
            </div>
            <div>
              <h3 className="text-base font-semibold text-slate-100">{config.actionTitle}</h3>
              <p className="text-xs text-slate-400 mt-0.5">Google Workspace User Confirmation</p>
            </div>
          </div>
          <button
            onClick={onClose}
            disabled={isLoading}
            className="text-slate-400 hover:text-slate-200 p-1 rounded-lg hover:bg-slate-800 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Content */}
        <div className="p-5 space-y-4">
          <p className="text-sm text-slate-300 leading-relaxed">{config.actionDescription}</p>

          {config.details.length > 0 && (
            <div className="bg-slate-950 border border-slate-800 rounded-lg p-3.5 space-y-2 font-mono text-xs">
              {config.details.map((item, idx) => (
                <div key={idx} className="flex justify-between items-start gap-4">
                  <span className="text-slate-500 shrink-0">{item.label}:</span>
                  <span className="text-slate-200 text-right truncate font-medium">{item.value}</span>
                </div>
              ))}
            </div>
          )}

          <div className="flex items-center gap-2 p-3 bg-amber-950/30 border border-amber-900/50 rounded-lg text-amber-300 text-xs">
            <ShieldAlert className="w-4 h-4 shrink-0 text-amber-400" />
            <span>
              This operation will modify data in your Google Account with your explicit permission.
            </span>
          </div>
        </div>

        {/* Actions */}
        <div className="p-4 border-t border-slate-800 bg-slate-950/40 flex items-center justify-end gap-2.5">
          <button
            type="button"
            onClick={onClose}
            disabled={isLoading}
            className="px-4 py-2 text-xs font-medium text-slate-400 hover:text-slate-200 hover:bg-slate-800 rounded-lg transition-colors border border-transparent"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={async () => {
              await config.onConfirm();
              onClose();
            }}
            disabled={isLoading}
            className={`px-4 py-2 text-xs font-semibold rounded-lg flex items-center gap-1.5 transition-all shadow-md ${
              config.isDestructive
                ? "bg-rose-600 hover:bg-rose-500 text-white"
                : "bg-indigo-600 hover:bg-indigo-500 text-white"
            }`}
          >
            {isLoading ? (
              <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
            ) : (
              <Check className="w-4 h-4" />
            )}
            <span>{config.confirmButtonText}</span>
          </button>
        </div>
      </div>
    </div>
  );
};

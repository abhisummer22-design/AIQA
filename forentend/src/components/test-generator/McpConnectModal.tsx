/**
 * AIQA — Skills & MCP, Phase 1: Connect MCP.
 *
 * Fresh, repository-wide inspection before writing this file (grep for
 * "mcp"/"MCP" across `services/orchestrator/src` and `forentend/src`, plus
 * a read of `SettingsPage.tsx` and `admin/api-keys*` — the only real
 * credential/config surfaces this backend has) confirms: there is no MCP
 * server configuration, connector backend, credential storage, plugin
 * registry, or model/provider-configuration architecture anywhere in this
 * repository. This modal therefore does not attempt to connect to
 * anything — it is a purely informational, honest disclosure, exactly as
 * brief §6 requires when no real MCP architecture exists.
 *
 * Deliberately collects zero input (no name field, no URL field, no
 * credential field): brief §6 explicitly forbids storing an arbitrary
 * MCP URL or API key in browser storage, and a form whose values are
 * accepted but then discarded (nothing here persists anything, even in
 * memory beyond this modal being open) would be more misleading than
 * informative. If a future unit adds a real connector, this is the single
 * place that needs to change.
 */
import React from 'react';
import { X, AlertTriangle, Radio } from 'lucide-react';
import { MCP_BACKEND_CONFIGURED } from '../../services/skillRegistry';

interface McpConnectModalProps {
  onClose: () => void;
}

export const McpConnectModal: React.FC<McpConnectModalProps> = ({ onClose }) => {
  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-16 bg-slate-950/80 backdrop-blur-sm p-4 animate-in fade-in duration-100"
      onClick={onClose}
    >
      <div
        id="mcp-connect-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Connect MCP"
        className="w-full max-w-lg rounded-lg bg-[#0B1120] border border-slate-700/80 shadow-2xl overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-slate-800 bg-[#020617]">
          <h2 className="text-sm font-bold text-slate-100 font-sans">Connect MCP</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close Connect MCP dialog"
            className="p-1.5 text-content-muted hover:text-slate-300 rounded cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-4 space-y-3">
          <div
            id="mcp-status-badge"
            className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-mono font-semibold bg-slate-900 border border-slate-700 text-slate-400"
          >
            <Radio className="w-3 h-3 text-content-muted" />
            <span>{MCP_BACKEND_CONFIGURED ? 'Connected' : 'Not Configured'}</span>
          </div>

          <div className="p-3 rounded-lg bg-amber-950/30 border border-amber-800/60 flex items-start gap-2.5">
            <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
            <p className="text-xs font-sans text-amber-200 leading-relaxed">
              External MCP connection management is not yet configured in the current AIQA backend. There is no MCP
              server registry, credential store, or connector endpoint to connect to today.
            </p>
          </div>

          <div className="text-xs font-sans text-slate-400 space-y-1.5">
            <p>When a real MCP connector is implemented, this dialog will let you:</p>
            <ul className="list-disc list-inside space-y-0.5 pl-1 text-slate-400">
              <li>Register a real MCP server endpoint</li>
              <li>Verify a live connection before showing it as Connected</li>
              <li>Discover and select real MCP-provided capabilities as skills</li>
            </ul>
          </div>

          <p className="text-[11px] font-mono text-content-muted">
            No connection information is collected or stored by this dialog.
          </p>
        </div>

        <div className="px-4 py-3 border-t border-slate-800 bg-[#020617] flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white font-mono text-xs font-bold cursor-pointer"
          >
            Got it
          </button>
        </div>
      </div>
    </div>
  );
};

import React from "react";
import { X, Bell, AlertTriangle, AlertOctagon, CheckCircle2, Trash2, ArrowRight } from "lucide-react";
import { AlertItem, ViewType } from "../../types";

interface AlertCenterProps {
  isOpen: boolean;
  onClose: () => void;
  alerts: AlertItem[];
  onDismissAlert: (id: string) => void;
  onNavigate: (view: ViewType) => void;
}

export const AlertCenter: React.FC<AlertCenterProps> = ({
  isOpen,
  onClose,
  alerts,
  onDismissAlert,
  onNavigate
}) => {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/60 backdrop-blur-xs">
      <div
        id="drawer-alert-center"
        className="w-full max-w-md bg-[#0e1422] border-l border-slate-800 h-full flex flex-col justify-between shadow-2xl animate-in slide-in-from-right duration-200"
      >
        {/* Header */}
        <div className="px-5 py-4 border-b border-slate-800 flex items-center justify-between bg-[#0b0f19]">
          <div className="flex items-center gap-2">
            <Bell className="w-4 h-4 text-cyan-400" />
            <h2 className="text-sm font-bold font-mono text-white">Alert Center & Telemetry</h2>
          </div>
          <button
            onClick={onClose}
            aria-label="Close alerts"
            className="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* List */}
        <div className="p-4 overflow-y-auto space-y-3 flex-1">
          {alerts.length === 0 ? (
            <div className="p-8 text-center text-slate-500 text-xs font-mono">
              <CheckCircle2 className="w-8 h-8 text-emerald-500/60 mx-auto mb-2" />
              All systems nominal. Zero active alerts.
            </div>
          ) : (
            alerts.map((alert) => (
              <div
                key={alert.id}
                className={`p-3.5 rounded-lg border text-xs font-mono space-y-1.5 ${
                  alert.level === "CRITICAL"
                    ? "bg-rose-950/40 border-rose-800/80 text-rose-200"
                    : alert.level === "DEGRADED"
                    ? "bg-amber-950/40 border-amber-800/80 text-amber-200"
                    : "bg-slate-900 border-slate-800 text-slate-300"
                }`}
              >
                <div className="flex items-center justify-between">
                  <span className="font-bold flex items-center gap-1.5">
                    {alert.level === "CRITICAL" ? (
                      <AlertOctagon className="w-3.5 h-3.5 text-rose-400" />
                    ) : (
                      <AlertTriangle className="w-3.5 h-3.5 text-amber-400" />
                    )}
                    {alert.title}
                  </span>
                  <span className="text-[10px] text-slate-400">{alert.timestamp}</span>
                </div>

                <p className="text-[11px] text-slate-300 leading-relaxed font-sans">{alert.message}</p>

                <div className="pt-1 flex items-center justify-between border-t border-slate-800/60">
                  <span className="text-[10px] text-slate-500">Component: {alert.component}</span>
                  <button
                    onClick={() => onDismissAlert(alert.id)}
                    className="text-[10px] text-slate-400 hover:text-slate-200 underline"
                  >
                    Dismiss
                  </button>
                </div>
              </div>
            ))
          )}
        </div>

        {/* Footer */}
        <div className="p-4 border-t border-slate-800 bg-[#0b0f19]">
          <button
            onClick={() => {
              onClose();
              onNavigate("audit-logs");
            }}
            className="w-full py-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-center text-slate-200 font-mono flex items-center justify-center gap-1.5"
          >
            <span>View Full Telemetry & Audit Logs</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
};

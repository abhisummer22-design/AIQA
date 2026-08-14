import React from "react";
import { RoleConfig } from "./types";
import { Maximize2, Shield, Radio, Terminal } from "lucide-react";
import { SessionStatus } from "./SessionStatus";

interface RoleHeaderProps {
  role: RoleConfig;
  isZoomed?: boolean;
  onZoom?: () => void;
}

export const RoleHeader: React.FC<RoleHeaderProps> = ({ role, isZoomed, onZoom }) => {
  return (
    <div className="flex items-start justify-between gap-2 p-3 bg-zinc-900/90 border-b border-zinc-800">
      <div className="flex items-center gap-2.5 min-w-0">
        <div
          className={`w-8 h-8 rounded-lg flex items-center justify-center font-bold text-xs shrink-0 border ${role.color.border} ${role.color.bg} ${role.color.text}`}
        >
          <Shield className="w-4 h-4" />
        </div>
        <div className="min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <h3 className="font-bold text-xs sm:text-sm tracking-wider uppercase text-zinc-100 font-mono">
              {role.name}
            </h3>
            <SessionStatus role={role} />
            <span className="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[9px] font-mono font-medium bg-zinc-800 text-zinc-400 border border-zinc-700/60">
              <Radio className="w-2.5 h-2.5 text-sky-400" />
              LIVE
            </span>
          </div>
          <div className="flex items-center gap-1.5 text-[11px] text-zinc-400 mt-0.5 truncate font-mono">
            <Terminal className="w-3 h-3 text-zinc-500 shrink-0" />
            <span className="truncate text-zinc-300 hover:text-sky-300 transition-colors">
              {role.email}
            </span>
          </div>
        </div>
      </div>

      {!isZoomed && onZoom && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onZoom();
          }}
          className="shrink-0 p-1.5 rounded-md text-zinc-400 hover:text-sky-300 hover:bg-zinc-800/80 transition-all border border-transparent hover:border-zinc-700"
          title="Zoom into role session"
        >
          <Maximize2 className="w-3.5 h-3.5" />
        </button>
      )}
    </div>
  );
};

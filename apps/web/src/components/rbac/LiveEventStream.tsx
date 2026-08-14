import React, { useState } from "react";
import { LiveRbacEvent, RoleId } from "./types";
import {
  Terminal,
  CheckCircle2,
  XCircle,
  Filter,
  Search,
  ExternalLink,
  Trash2,
  Pause,
  Play,
  Clock,
  Sparkles,
  Info
} from "lucide-react";

interface LiveEventStreamProps {
  events: LiveRbacEvent[];
  onSelectEvent: (event: LiveRbacEvent) => void;
  onClearEvents: () => void;
  onInspectEvent: (event: LiveRbacEvent) => void;
}

export const LiveEventStream: React.FC<LiveEventStreamProps> = ({
  events,
  onSelectEvent,
  onClearEvents,
  onInspectEvent
}) => {
  const [filterResult, setFilterResult] = useState<"all" | "AUTHORIZED" | "FORBIDDEN">("all");
  const [filterRole, setFilterRole] = useState<string>("all");
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [isPaused, setIsPaused] = useState<boolean>(false);

  const filteredEvents = events.filter((e) => {
    if (filterResult !== "all" && e.result !== filterResult) return false;
    if (filterRole !== "all" && e.roleId !== filterRole) return false;
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      return (
        e.action.toLowerCase().includes(q) ||
        e.permission.toLowerCase().includes(q) ||
        e.actorEmail.toLowerCase().includes(q) ||
        e.resource.toLowerCase().includes(q)
      );
    }
    return true;
  });

  const getRoleBadge = (roleId: RoleId) => {
    switch (roleId) {
      case "super_admin":
        return <span className="px-1.5 py-0.2 rounded text-[9px] font-bold font-mono bg-purple-500/20 text-purple-300 border border-purple-500/30">SUPER ADMIN</span>;
      case "org_admin":
        return <span className="px-1.5 py-0.2 rounded text-[9px] font-bold font-mono bg-sky-500/20 text-sky-300 border border-sky-500/30">ADMIN</span>;
      case "project_admin":
        return <span className="px-1.5 py-0.2 rounded text-[9px] font-bold font-mono bg-indigo-500/20 text-indigo-300 border border-indigo-500/30">PROJECT MGR</span>;
      case "qa_engineer":
        return <span className="px-1.5 py-0.2 rounded text-[9px] font-bold font-mono bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">QA USER</span>;
    }
  };

  return (
    <div className="flex flex-col bg-zinc-950 rounded-xl border border-zinc-800/90 overflow-hidden shadow-2xl">
      {/* Header Bar */}
      <div className="flex items-center justify-between px-3.5 py-2.5 bg-zinc-900/90 border-b border-zinc-800">
        <div className="flex items-center gap-2">
          <Terminal className="w-4 h-4 text-sky-400" />
          <h3 className="font-mono font-bold text-xs tracking-wider uppercase text-zinc-100 flex items-center gap-2">
            LIVE RBAC EVENTS
            <span className="px-1.5 py-0.2 rounded-full text-[10px] bg-zinc-800 text-zinc-400 font-normal">
              {events.length} total
            </span>
          </h3>
        </div>

        {/* Filter Controls */}
        <div className="flex items-center gap-2 font-mono text-[11px]">
          <div className="flex items-center bg-zinc-900 border border-zinc-800 rounded-md px-2 py-0.5">
            <Filter className="w-3 h-3 text-zinc-500 mr-1.5" />
            <select
              value={filterRole}
              onChange={(e) => setFilterRole(e.target.value)}
              className="bg-transparent text-zinc-300 text-xs focus:outline-none cursor-pointer"
            >
              <option value="all">All Roles</option>
              <option value="super_admin">Super Admin</option>
              <option value="org_admin">Admin</option>
              <option value="project_admin">Project Manager</option>
              <option value="qa_engineer">QA User</option>
            </select>
          </div>

          <div className="flex items-center bg-zinc-900 border border-zinc-800 rounded-md px-2 py-0.5">
            <select
              value={filterResult}
              onChange={(e) => setFilterResult(e.target.value as any)}
              className="bg-transparent text-zinc-300 text-xs focus:outline-none cursor-pointer"
            >
              <option value="all">All Results</option>
              <option value="AUTHORIZED">Authorized Only</option>
              <option value="FORBIDDEN">Forbidden (403)</option>
            </select>
          </div>

          <button
            onClick={() => setIsPaused(!isPaused)}
            className={`p-1.5 rounded-md border transition-colors ${
              isPaused
                ? "bg-amber-500/20 text-amber-300 border-amber-500/40"
                : "bg-zinc-800 text-zinc-400 hover:text-zinc-200 border-zinc-700"
            }`}
            title={isPaused ? "Resume Stream" : "Pause Stream"}
          >
            {isPaused ? <Play className="w-3 h-3" /> : <Pause className="w-3 h-3" />}
          </button>

          <button
            onClick={onClearEvents}
            className="p-1.5 rounded-md bg-zinc-800 text-zinc-400 hover:text-rose-300 hover:bg-rose-950/40 border border-zinc-700 hover:border-rose-800 transition-colors"
            title="Clear Stream History"
          >
            <Trash2 className="w-3 h-3" />
          </button>
        </div>
      </div>

      {/* Events Log Console */}
      <div className="max-h-52 overflow-y-auto divide-y divide-zinc-900 font-mono text-[11px]">
        {filteredEvents.length === 0 ? (
          <div className="p-6 text-center text-zinc-500 flex flex-col items-center justify-center gap-1.5">
            <Clock className="w-5 h-5 text-zinc-600" />
            <p>No RBAC authorization events match current filter</p>
            <p className="text-[10px] text-zinc-600">Click any action button in the role panels above to simulate authorization flows</p>
          </div>
        ) : (
          filteredEvents.map((evt) => {
            const isAuth = evt.result === "AUTHORIZED";
            return (
              <div
                key={evt.id}
                onClick={() => onSelectEvent(evt)}
                className={`group flex items-center justify-between px-3.5 py-2 hover:bg-zinc-900/90 transition-colors cursor-pointer ${
                  isAuth
                    ? "hover:border-l-2 hover:border-l-sky-400"
                    : "hover:border-l-2 hover:border-l-orange-400"
                }`}
              >
                {/* Left: Timestamp & Role & Action */}
                <div className="flex items-center gap-2.5 truncate min-w-0 pr-2">
                  <span className="text-zinc-500 text-[10px] shrink-0">{evt.timeFormatted}</span>
                  {getRoleBadge(evt.roleId)}
                  <span className="text-zinc-400">→</span>
                  <span className="font-semibold text-zinc-200 truncate">{evt.action}</span>
                  <span className="text-zinc-500 text-[10px] hidden md:inline truncate">
                    ({evt.permission})
                  </span>
                  <span className="text-zinc-600 text-[10px] hidden lg:inline truncate">
                    [{evt.resource}]
                  </span>
                </div>

                {/* Right: Verdict Badge & Inspect */}
                <div className="flex items-center gap-2 shrink-0">
                  <span className="text-zinc-500 text-[10px] hidden sm:inline font-mono">
                    {evt.latencyMs}ms
                  </span>

                  <span
                    className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-bold border ${
                      isAuth
                        ? "bg-sky-500/10 text-sky-300 border-sky-500/30"
                        : "bg-orange-500/10 text-orange-300 border-orange-500/30"
                    }`}
                  >
                    {isAuth ? (
                      <>
                        <CheckCircle2 className="w-3 h-3 text-sky-400" />
                        AUTHORIZED
                      </>
                    ) : (
                      <>
                        <XCircle className="w-3 h-3 text-orange-400" />
                        403 FORBIDDEN
                      </>
                    )}
                  </span>

                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onInspectEvent(evt);
                    }}
                    className="opacity-0 group-hover:opacity-100 p-1 text-zinc-400 hover:text-sky-300 transition-opacity"
                    title="Inspect authorization payload details"
                  >
                    <Info className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};

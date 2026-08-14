import React from "react";
import {
  LayoutDashboard,
  FolderKanban,
  PlayCircle,
  CheckSquare,
  Network,
  Smartphone,
  KeyRound,
  Cpu,
  GitGraph,
  FileCheck2,
  Bug,
  AlertTriangle,
  ShieldCheck,
  History,
  FileBarChart,
  Settings,
  ChevronLeft,
  ChevronRight,
  Activity,
  User,
  Building2,
  LogOut,
  Sparkles,
  Server,
  Mail,
  Brain
} from "lucide-react";
import { ViewType, SystemHealthState } from "../../types";

interface SidebarProps {
  currentView?: ViewType;
  activeView?: ViewType;
  onNavigate?: (view: ViewType) => void;
  onSelectView?: (view: ViewType) => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  systemHealth: SystemHealthState;
  onOpenDiagnostics: () => void;
  tenantName?: string;
  userName?: string;
  userRole?: string;
}

const navItems: { id: ViewType; label: string; icon: React.ComponentType<{ className?: string }>; badge?: string }[] = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "ai-brain", label: "Main Brain (AI Power)", icon: Brain, badge: "Ollama/API" },
  { id: "projects", label: "Projects", icon: FolderKanban, badge: "4" },
  { id: "test-runs", label: "Test Case Execution Hub", icon: PlayCircle, badge: "Live" },
  { id: "test-cases", label: "Test Cases", icon: CheckSquare },
  { id: "test-case-generator", label: "Test Case Generator", icon: Sparkles, badge: "AI Gen" },
  { id: "api-testing", label: "API Testing", icon: Network },
  { id: "mobile-testing", label: "Mobile Testing", icon: Smartphone, badge: "Devices" },
  { id: "rbac-testing", label: "RBAC Testing", icon: KeyRound, badge: "SecOps" },
  { id: "workspace", label: "Google Workspace", icon: Mail, badge: "Gmail/Sheets" },
  { id: "bugs", label: "Bugs", icon: Bug, badge: "3 New" },
  { id: "risk-flakiness", label: "Risk & Flakiness", icon: AlertTriangle },
  { id: "policies", label: "Policies", icon: ShieldCheck },
  { id: "audit-logs", label: "Audit Logs", icon: History },
  { id: "reports", label: "Reports", icon: FileBarChart },
  { id: "settings", label: "Settings", icon: Settings },
];

export const Sidebar: React.FC<SidebarProps> = ({
  currentView,
  activeView,
  onNavigate,
  onSelectView,
  collapsed = false,
  onToggleCollapse,
  systemHealth,
  onOpenDiagnostics,
  tenantName = "Enterprise Cloud",
  userName = "Lead QA Engineer",
  userRole = "SecOps / Admin"
}) => {
  const selectedView = currentView || activeView || "dashboard";
  const handleSelect = (view: ViewType) => {
    if (onNavigate) onNavigate(view);
    if (onSelectView) onSelectView(view);
  };

  const isHealthy =
    systemHealth.backend === "online" &&
    systemHealth.database === "connected" &&
    systemHealth.ollama !== "offline";

  return (
    <aside
      id="sidebar-navigation"
      className={`fixed top-0 left-0 bottom-0 z-30 bg-[#0D0D0E] border-r border-slate-800 transition-all duration-200 flex flex-col justify-between select-none ${
        collapsed ? "w-14" : "w-52"
      }`}
    >
      {/* Brand Header */}
      <div>
        <div className="p-3 border-b border-slate-800 bg-slate-900/50 flex items-center justify-between">
          {!collapsed ? (
            <div className="flex items-center gap-2 overflow-hidden">
              <div className="w-6 h-6 bg-indigo-600 rounded flex items-center justify-center text-white font-bold text-xs shrink-0 shadow-sm">
                AI
              </div>
              <div className="flex flex-col">
                <span className="font-bold text-white tracking-tight text-xs flex items-center gap-1.5">
                  AIQA CORE <span className="text-[9px] px-1 py-0.2 rounded bg-indigo-950 text-indigo-300 border border-indigo-800/60 font-mono">v2.4</span>
                </span>
                <span className="text-[10px] text-slate-400 truncate opacity-60">Quality Operations</span>
              </div>
            </div>
          ) : (
            <div className="w-6 h-6 bg-indigo-600 rounded flex items-center justify-center text-white font-bold text-xs mx-auto">
              AI
            </div>
          )}

          {onToggleCollapse && (
            <button
              id="btn-toggle-sidebar"
              onClick={onToggleCollapse}
              aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
              className="p-1 rounded text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition-colors"
            >
              {collapsed ? <ChevronRight className="w-3.5 h-3.5" /> : <ChevronLeft className="w-3.5 h-3.5" />}
            </button>
          )}
        </div>

        {/* Navigation List */}
        <nav className="p-2 space-y-0.5 overflow-y-auto max-h-[calc(100vh-220px)] text-[11px] font-medium">
          {navItems.map((item) => {
            const Icon = item.icon;
            const isActive = selectedView === item.id;
            return (
              <button
                key={item.id}
                id={`nav-item-${item.id}`}
                onClick={() => handleSelect(item.id)}
                title={collapsed ? item.label : undefined}
                className={`w-full flex items-center gap-2.5 px-2.5 py-1.5 rounded transition-colors text-left ${
                  isActive
                    ? "bg-indigo-600/15 text-indigo-400 border border-indigo-500/30 font-semibold"
                    : "text-slate-300 opacity-70 hover:opacity-100 hover:bg-slate-800/60 border border-transparent"
                }`}
              >
                <Icon className={`w-3.5 h-3.5 shrink-0 ${isActive ? "text-indigo-400" : "text-slate-400"}`} />
                {!collapsed && (
                  <div className="flex-1 flex items-center justify-between truncate">
                    <span className="truncate">{item.label}</span>
                    {item.badge && (
                      <span
                        className={`text-[9px] px-1.5 py-0.2 rounded font-mono uppercase tracking-wider ${
                          item.badge === "Live"
                            ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 animate-pulse"
                            : item.badge.includes("New")
                            ? "bg-amber-500/10 text-amber-400 border border-amber-500/30"
                            : "bg-slate-800 text-slate-400"
                        }`}
                      >
                        {item.badge}
                      </span>
                    )}
                  </div>
                )}
              </button>
            );
          })}
        </nav>
      </div>

      {/* Bottom User & Health Widget */}
      <div className="p-2.5 border-t border-slate-800 space-y-1.5 bg-[#09090A] text-[10px]">
        {/* System Health Quick Card */}
        <button
          id="btn-sidebar-system-health"
          onClick={onOpenDiagnostics}
          title={collapsed ? "System Diagnostics" : undefined}
          className={`w-full p-2 rounded border text-left transition-all ${
            isHealthy
              ? "bg-emerald-500/10 border-emerald-500/30 hover:bg-emerald-500/20 text-emerald-300"
              : "bg-rose-500/10 border-rose-500/30 hover:bg-rose-500/20 text-rose-300"
          }`}
        >
          <div className="flex items-center gap-2">
            <span className="relative flex h-2 w-2 shrink-0">
              <span
                className={`animate-ping absolute inline-flex h-full w-full rounded-full opacity-75 ${
                  isHealthy ? "bg-emerald-400" : "bg-rose-400"
                }`}
              />
              <span
                className={`relative inline-flex rounded-full h-2 w-2 ${
                  isHealthy ? "bg-emerald-500" : "bg-rose-500"
                }`}
              />
            </span>
            {!collapsed && (
              <div className="flex-1 truncate">
                <div className="text-[10px] font-bold tracking-wider font-mono uppercase">
                  {isHealthy ? "Cluster Online" : "Degraded State"}
                </div>
                <div className="text-[9px] text-slate-400 flex items-center justify-between mt-0.5">
                  <span>99.98% SLA</span>
                  <span className="text-indigo-400 hover:underline">Diagnostics &rarr;</span>
                </div>
              </div>
            )}
          </div>
        </button>

        {/* Tenant & User Info */}
        {!collapsed ? (
          <div className="pt-1 flex items-center justify-between text-slate-400">
            <div className="flex items-center gap-2 overflow-hidden">
              <div className="w-6 h-6 rounded bg-slate-800 border border-slate-700 flex items-center justify-center text-slate-200 text-[10px] shrink-0 font-bold">
                {userName.charAt(0)}
              </div>
              <div className="overflow-hidden">
                <div className="text-[11px] font-medium text-slate-200 truncate">{userName}</div>
                <div className="text-[9px] text-slate-500 truncate flex items-center gap-1 font-mono">
                  {tenantName}
                </div>
              </div>
            </div>
            <button
              id="btn-sidebar-logout"
              title="Workspace RBAC active"
              onClick={() => alert("Session active. Multi-tenant RBAC enforced for " + tenantName)}
              className="p-1 text-slate-500 hover:text-slate-300 hover:bg-slate-800 rounded transition-colors"
            >
              <LogOut className="w-3 h-3" />
            </button>
          </div>
        ) : (
          <div className="flex justify-center">
            <div className="w-6 h-6 rounded bg-slate-800 border border-slate-700 flex items-center justify-center text-slate-200 text-[10px] font-bold">
              {userName.charAt(0)}
            </div>
          </div>
        )}
      </div>
    </aside>
  );
};


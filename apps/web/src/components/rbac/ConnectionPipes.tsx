import React, { useState } from "react";
import { RoleId, EventResult } from "./types";
import { Zap, Eye, Terminal } from "lucide-react";

interface ConnectionPipesProps {
  activeRoleId?: RoleId | null;
  activeResult?: EventResult | null;
  isEvaluating?: boolean;
  onSelectPipe?: (roleId: RoleId) => void;
}

export const ConnectionPipes: React.FC<ConnectionPipesProps> = ({
  activeRoleId,
  activeResult,
  isEvaluating,
  onSelectPipe
}) => {
  const [hoveredPipe, setHoveredPipe] = useState<RoleId | null>(null);

  // We render an SVG overlay with 4 main pipe branches from the corners towards center
  // Top-Left (Super Admin): (22%, 22%) -> (50%, 50%)
  // Top-Right (Admin): (78%, 22%) -> (50%, 50%)
  // Bottom-Left (PM): (22%, 78%) -> (50%, 50%)
  // Bottom-Right (User): (78%, 78%) -> (50%, 50%)

  const getPipeStyles = (roleId: RoleId) => {
    const isActive = activeRoleId === roleId;
    const isHovered = hoveredPipe === roleId;

    if (isHovered) {
      return {
        stroke: "#38bdf8",
        strokeWidth: 3,
        strokeDasharray: "none",
        opacity: 1,
        filter: "drop-shadow(0 0 10px #38bdf8)",
        cursor: "pointer"
      };
    }

    if (!isActive) {
      return {
        stroke: "#3f3f46", // zinc-700
        strokeWidth: 1.5,
        strokeDasharray: "4 4",
        opacity: 0.4,
        filter: "none"
      };
    }

    if (activeResult === "AUTHORIZED") {
      return {
        stroke: "#38bdf8", // sky-400
        strokeWidth: 3.5,
        strokeDasharray: "none",
        opacity: 1,
        filter: "drop-shadow(0 0 10px #38bdf8)"
      };
    }

    if (activeResult === "FORBIDDEN") {
      return {
        stroke: "#f97316", // orange-500
        strokeWidth: 3.5,
        strokeDasharray: "none",
        opacity: 1,
        filter: "drop-shadow(0 0 10px #f97316)"
      };
    }

    // Default evaluating state
    return {
      stroke: "#38bdf8",
      strokeWidth: 2.5,
      strokeDasharray: "6 4",
      opacity: 0.9,
      filter: "drop-shadow(0 0 6px #38bdf8)"
    };
  };

  const getParticleColor = (roleId: RoleId) => {
    if (activeRoleId !== roleId) return null;
    return activeResult === "FORBIDDEN" ? "#ff6b00" : "#00f0ff";
  };

  const handlePipeClick = (roleId: RoleId) => {
    if (onSelectPipe) {
      onSelectPipe(roleId);
    }
  };

  return (
    <div className="absolute inset-0 w-full h-full pointer-events-none z-10">
      <svg
        className="w-full h-full"
        xmlns="http://www.w3.org/2000/svg"
      >
        <defs>
          {/* Glow Filters */}
          <filter id="neon-glow-blue" x="-20%" y="-20%" width="140%" height="140%">
            <feGaussianBlur stdDeviation="4" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
          <filter id="neon-glow-orange" x="-20%" y="-20%" width="140%" height="140%">
            <feGaussianBlur stdDeviation="4" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>

        {/* ----------------- Pipe 1: Top-Left (Super Admin) to Center ----------------- */}
        {/* Invisible wider hit path for easy clicking */}
        <path
          d="M 22% 22% C 35% 22%, 35% 50%, 50% 50%"
          fill="none"
          stroke="transparent"
          strokeWidth="32"
          className="cursor-pointer pointer-events-auto"
          onMouseEnter={() => setHoveredPipe("super_admin")}
          onMouseLeave={() => setHoveredPipe(null)}
          onClick={() => handlePipeClick("super_admin")}
        >
          <title>Click to inspect Super Admin Flow JSON payload</title>
        </path>
        <path
          id="path-super-admin"
          d="M 22% 22% C 35% 22%, 35% 50%, 50% 50%"
          fill="none"
          style={getPipeStyles("super_admin")}
          className="pointer-events-none transition-all duration-200"
        />
        {activeRoleId === "super_admin" && (
          <circle r="5" fill={getParticleColor("super_admin") || "#38bdf8"}>
            <animateMotion
              dur="1.2s"
              repeatCount="indefinite"
              path="M 22% 22% C 35% 22%, 35% 50%, 50% 50%"
            />
          </circle>
        )}

        {/* ----------------- Pipe 2: Top-Right (Admin) to Center ----------------- */}
        <path
          d="M 78% 22% C 65% 22%, 65% 50%, 50% 50%"
          fill="none"
          stroke="transparent"
          strokeWidth="32"
          className="cursor-pointer pointer-events-auto"
          onMouseEnter={() => setHoveredPipe("org_admin")}
          onMouseLeave={() => setHoveredPipe(null)}
          onClick={() => handlePipeClick("org_admin")}
        >
          <title>Click to inspect Admin Flow JSON payload</title>
        </path>
        <path
          id="path-admin"
          d="M 78% 22% C 65% 22%, 65% 50%, 50% 50%"
          fill="none"
          style={getPipeStyles("org_admin")}
          className="pointer-events-none transition-all duration-200"
        />
        {activeRoleId === "org_admin" && (
          <circle r="5" fill={getParticleColor("org_admin") || "#38bdf8"}>
            <animateMotion
              dur="1.2s"
              repeatCount="indefinite"
              path="M 78% 22% C 65% 22%, 65% 50%, 50% 50%"
            />
          </circle>
        )}

        {/* ----------------- Pipe 3: Bottom-Left (Project Manager) to Center ----------------- */}
        <path
          d="M 22% 78% C 35% 78%, 35% 50%, 50% 50%"
          fill="none"
          stroke="transparent"
          strokeWidth="32"
          className="cursor-pointer pointer-events-auto"
          onMouseEnter={() => setHoveredPipe("project_admin")}
          onMouseLeave={() => setHoveredPipe(null)}
          onClick={() => handlePipeClick("project_admin")}
        >
          <title>Click to inspect Project Manager Flow JSON payload</title>
        </path>
        <path
          id="path-pm"
          d="M 22% 78% C 35% 78%, 35% 50%, 50% 50%"
          fill="none"
          style={getPipeStyles("project_admin")}
          className="pointer-events-none transition-all duration-200"
        />
        {activeRoleId === "project_admin" && (
          <circle r="5" fill={getParticleColor("project_admin") || "#38bdf8"}>
            <animateMotion
              dur="1.2s"
              repeatCount="indefinite"
              path="M 22% 78% C 35% 78%, 35% 50%, 50% 50%"
            />
          </circle>
        )}

        {/* ----------------- Pipe 4: Bottom-Right (User / QA User) to Center ----------------- */}
        <path
          d="M 78% 78% C 65% 78%, 65% 50%, 50% 50%"
          fill="none"
          stroke="transparent"
          strokeWidth="32"
          className="cursor-pointer pointer-events-auto"
          onMouseEnter={() => setHoveredPipe("qa_engineer")}
          onMouseLeave={() => setHoveredPipe(null)}
          onClick={() => handlePipeClick("qa_engineer")}
        >
          <title>Click to inspect QA User Flow JSON payload</title>
        </path>
        <path
          id="path-user"
          d="M 78% 78% C 65% 78%, 65% 50%, 50% 50%"
          fill="none"
          style={getPipeStyles("qa_engineer")}
          className="pointer-events-none transition-all duration-200"
        />
        {activeRoleId === "qa_engineer" && (
          <circle r="5" fill={getParticleColor("qa_engineer") || "#38bdf8"}>
            <animateMotion
              dur="1.2s"
              repeatCount="indefinite"
              path="M 78% 78% C 65% 78%, 65% 50%, 50% 50%"
            />
          </circle>
        )}
      </svg>

      {/* Interactive Midpoint Flow Badges overlay (HTML Buttons positioned over curves) */}
      <div className="hidden lg:block">
        {/* Pipe 1 Badge: Super Admin */}
        <div
          className="absolute top-[32%] left-[30%] -translate-x-1/2 -translate-y-1/2 pointer-events-auto z-20 group"
          onMouseEnter={() => setHoveredPipe("super_admin")}
          onMouseLeave={() => setHoveredPipe(null)}
        >
          <button
            onClick={() => handlePipeClick("super_admin")}
            className="flex items-center gap-1.5 px-2 py-1 rounded-full text-[9px] font-mono font-bold bg-zinc-900/90 text-purple-300 border border-purple-500/40 hover:border-purple-400 hover:bg-purple-950/60 hover:text-white shadow-lg backdrop-blur-sm transition-all group-hover:scale-105 active:scale-95"
            title="Inspect Super Admin Flow JSON Payload"
          >
            <Zap className="w-2.5 h-2.5 text-purple-400" />
            <span>PIPE 1: FLOW INSPECT</span>
            <Eye className="w-2.5 h-2.5 opacity-60 group-hover:opacity-100" />
          </button>
        </div>

        {/* Pipe 2 Badge: Admin */}
        <div
          className="absolute top-[32%] left-[70%] -translate-x-1/2 -translate-y-1/2 pointer-events-auto z-20 group"
          onMouseEnter={() => setHoveredPipe("org_admin")}
          onMouseLeave={() => setHoveredPipe(null)}
        >
          <button
            onClick={() => handlePipeClick("org_admin")}
            className="flex items-center gap-1.5 px-2 py-1 rounded-full text-[9px] font-mono font-bold bg-zinc-900/90 text-sky-300 border border-sky-500/40 hover:border-sky-400 hover:bg-sky-950/60 hover:text-white shadow-lg backdrop-blur-sm transition-all group-hover:scale-105 active:scale-95"
            title="Inspect Admin Flow JSON Payload"
          >
            <Zap className="w-2.5 h-2.5 text-sky-400" />
            <span>PIPE 2: FLOW INSPECT</span>
            <Eye className="w-2.5 h-2.5 opacity-60 group-hover:opacity-100" />
          </button>
        </div>

        {/* Pipe 3 Badge: Project Manager */}
        <div
          className="absolute top-[68%] left-[30%] -translate-x-1/2 -translate-y-1/2 pointer-events-auto z-20 group"
          onMouseEnter={() => setHoveredPipe("project_admin")}
          onMouseLeave={() => setHoveredPipe(null)}
        >
          <button
            onClick={() => handlePipeClick("project_admin")}
            className="flex items-center gap-1.5 px-2 py-1 rounded-full text-[9px] font-mono font-bold bg-zinc-900/90 text-indigo-300 border border-indigo-500/40 hover:border-indigo-400 hover:bg-indigo-950/60 hover:text-white shadow-lg backdrop-blur-sm transition-all group-hover:scale-105 active:scale-95"
            title="Inspect Project Manager Flow JSON Payload"
          >
            <Zap className="w-2.5 h-2.5 text-indigo-400" />
            <span>PIPE 3: FLOW INSPECT</span>
            <Eye className="w-2.5 h-2.5 opacity-60 group-hover:opacity-100" />
          </button>
        </div>

        {/* Pipe 4 Badge: QA User */}
        <div
          className="absolute top-[68%] left-[70%] -translate-x-1/2 -translate-y-1/2 pointer-events-auto z-20 group"
          onMouseEnter={() => setHoveredPipe("qa_engineer")}
          onMouseLeave={() => setHoveredPipe(null)}
        >
          <button
            onClick={() => handlePipeClick("qa_engineer")}
            className="flex items-center gap-1.5 px-2 py-1 rounded-full text-[9px] font-mono font-bold bg-zinc-900/90 text-emerald-300 border border-emerald-500/40 hover:border-emerald-400 hover:bg-emerald-950/60 hover:text-white shadow-lg backdrop-blur-sm transition-all group-hover:scale-105 active:scale-95"
            title="Inspect QA User Flow JSON Payload"
          >
            <Zap className="w-2.5 h-2.5 text-emerald-400" />
            <span>PIPE 4: FLOW INSPECT</span>
            <Eye className="w-2.5 h-2.5 opacity-60 group-hover:opacity-100" />
          </button>
        </div>
      </div>
    </div>
  );
};


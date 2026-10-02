import React from 'react';
import { motion } from 'motion/react';
import { ProjectStatus } from '../../types';
import { Sparkles, ShieldCheck, Activity, AlertTriangle, Moon } from 'lucide-react';
import { usePrefersReducedMotion } from '../../hooks/usePrefersReducedMotion';

interface AIQualityCoreOrbProps {
  status: ProjectStatus;
  hasActiveRun: boolean;
  projectName: string;
}

export const AIQualityCoreOrb: React.FC<AIQualityCoreOrbProps> = ({
  status,
  hasActiveRun,
  projectName,
}) => {
  // Every animation in this component is framer-motion `repeat: Infinity`, and
  // framer-motion drives its own rAF loop rather than CSS animation properties,
  // so the blanket `prefers-reduced-motion` rule in `index.css` cannot reach it.
  // This has to be a JS decision.
  //
  // The rings collapse to a fixed angle rather than to zero, so the layout the
  // eye was reading does not jump — the ring is still visibly a ring, in a
  // defined orientation. The core stops pulsing at the midpoint of its range,
  // which is `scale: 1, opacity: 0.85`: still fully present, just no longer
  // breathing. Collapsing the core to `opacity: 0` instead would have hidden the
  // AIQA branding inside it, and that text is content, not decoration.
  const reducedMotion = usePrefersReducedMotion();
  // Determine color scheme based on project status and active execution
  const getTheme = () => {
    if (status === 'Offline') {
      return {
        coreGlow: 'from-slate-600/30 via-slate-800/40 to-transparent',
        innerCore: 'from-slate-400 via-slate-600 to-slate-800',
        ringBorder1: 'border-slate-700/40',
        ringBorder2: 'border-slate-600/30',
        ringBorder3: 'border-slate-800/60',
        accentText: 'text-slate-400',
        badgeBg: 'bg-slate-900/80 border-slate-700 text-slate-400',
        statusLabel: 'IDLE STANDBY',
        icon: Moon,
      };
    }
    if (status === 'Warning' || status === 'Degraded') {
      return {
        coreGlow: 'from-amber-500/25 via-amber-600/15 to-transparent',
        innerCore: 'from-amber-300 via-amber-500 to-orange-600',
        ringBorder1: 'border-amber-500/40',
        ringBorder2: 'border-amber-400/30',
        ringBorder3: 'border-orange-500/30',
        accentText: 'text-amber-400',
        badgeBg: 'bg-amber-950/80 border-amber-800/80 text-amber-300',
        statusLabel: 'ANOMALY DETECTED',
        icon: AlertTriangle,
      };
    }
    if (status === 'Critical') {
      return {
        coreGlow: 'from-rose-500/30 via-rose-600/20 to-transparent',
        innerCore: 'from-rose-400 via-rose-600 to-red-800',
        ringBorder1: 'border-rose-500/50',
        ringBorder2: 'border-rose-400/40',
        ringBorder3: 'border-red-600/30',
        accentText: 'text-rose-400',
        badgeBg: 'bg-rose-950/80 border-rose-800/80 text-rose-300',
        statusLabel: 'CRITICAL STATE',
        icon: AlertTriangle,
      };
    }
    if (hasActiveRun || status === 'Running') {
      return {
        coreGlow: 'from-yellow-500/30 via-violet-600/25 to-transparent',
        innerCore: 'from-yellow-300 via-violet-400 to-purple-600',
        ringBorder1: 'border-yellow-400/50',
        ringBorder2: 'border-violet-400/40',
        ringBorder3: 'border-yellow-500/30',
        accentText: 'text-yellow-400',
        badgeBg: 'bg-yellow-950/80 border-yellow-700/80 text-yellow-300',
        statusLabel: 'ACTIVE INFERENCE',
        icon: Activity,
      };
    }
    // Healthy default
    return {
      coreGlow: 'from-emerald-500/25 via-yellow-600/20 to-transparent',
      innerCore: 'from-emerald-300 via-teal-400 to-yellow-600',
      ringBorder1: 'border-emerald-400/40',
      ringBorder2: 'border-teal-400/30',
      ringBorder3: 'border-emerald-600/30',
      accentText: 'text-emerald-400',
      badgeBg: 'bg-emerald-950/80 border-emerald-800/80 text-emerald-300',
      statusLabel: 'CORE STABLE',
      icon: ShieldCheck,
    };
  };

  const theme = getTheme();
  const StatusIcon = theme.icon;

  return (
    <div
      id="ai-quality-core-orb-container"
      className="relative flex flex-col items-center justify-center h-full min-h-[280px] p-4 overflow-hidden rounded-lg bg-gradient-to-b from-[#0B1120] via-[#070D19] to-[#020617] border border-slate-800/90 shadow-inner"
    >
      {/* Background Subtle Coordinate Grid */}
      <div
        className="absolute inset-0 opacity-[0.07] pointer-events-none"
        style={{
          backgroundImage:
            'radial-gradient(circle at center, #8b5cf6 1px, transparent 1px), linear-gradient(to right, #1e293b 1px, transparent 1px), linear-gradient(to bottom, #1e293b 1px, transparent 1px)',
          backgroundSize: '24px 24px, 48px 48px, 48px 48px',
        }}
      />

      {/* Atmospheric Volumetric Core Glow */}
      <div
        className={`absolute w-64 h-64 rounded-full bg-gradient-to-r ${theme.coreGlow} blur-3xl opacity-60 pointer-events-none transition-all duration-700`}
      />

      {/* 3D Holographic Stage */}
      <div className="relative w-48 h-48 flex items-center justify-center my-2">
        {/* Outer Elliptical Orbital Ring 1 */}
        <motion.div
          animate={{ rotate: reducedMotion ? 45 : 360 }}
          transition={
            reducedMotion
              ? { duration: 0 }
              : { duration: 24, repeat: Infinity, ease: 'linear' }
          }
          className={`absolute w-44 h-44 rounded-full border border-dashed ${theme.ringBorder1} opacity-70`}
          style={{ transformStyle: 'preserve-3d', transform: 'rotateX(68deg)' }}
        >
          {/* Orbital Satellite Bead 1 */}
          <div className="absolute -top-1 left-1/2 -translate-x-1/2 w-2.5 h-2.5 rounded-full bg-yellow-300 shadow-[0_0_8px_#facc15]" />
        </motion.div>

        {/* Outer Elliptical Orbital Ring 2 (Counter-Rotating) */}
        <motion.div
          animate={{ rotate: reducedMotion ? -30 : -360 }}
          transition={
            reducedMotion
              ? { duration: 0 }
              : { duration: 18, repeat: Infinity, ease: 'linear' }
          }
          className={`absolute w-40 h-40 rounded-full border ${theme.ringBorder2} opacity-60`}
          style={{ transformStyle: 'preserve-3d', transform: 'rotateX(65deg) rotateY(35deg)' }}
        >
          {/* Orbital Satellite Bead 2 */}
          <div className="absolute -bottom-1 left-1/2 -translate-x-1/2 w-2 h-2 rounded-full bg-violet-300 shadow-[0_0_8px_#a78bfa]" />
        </motion.div>

        {/* Equatorial Energy Horizon Ring */}
        <motion.div
          animate={{ rotate: reducedMotion ? 20 : 360 }}
          transition={
            reducedMotion
              ? { duration: 0 }
              : { duration: 30, repeat: Infinity, ease: 'linear' }
          }
          className={`absolute w-36 h-36 rounded-full border border-dotted ${theme.ringBorder3} opacity-80`}
          style={{ transformStyle: 'preserve-3d', transform: 'rotateX(78deg) rotateY(-20deg)' }}
        />

        {/* Layered Transparent Sphere Glass Shell */}
        <div className="relative w-28 h-28 rounded-full bg-gradient-to-br from-white/10 via-slate-900/80 to-black/90 backdrop-blur-md border border-white/20 shadow-[0_0_30px_rgba(139,92,246,0.25)] flex items-center justify-center overflow-hidden">
          {/* Top-Left Specular Glass Highlight */}
          <div className="absolute top-1.5 left-2 w-12 h-6 rounded-full bg-gradient-to-b from-white/40 to-transparent rotate-[-30deg] pointer-events-none blur-[0.5px]" />
          
          {/* Bottom Rim Ambient Reflection */}
          <div className="absolute bottom-1 right-2 w-10 h-4 rounded-full bg-yellow-400/20 blur-sm pointer-events-none" />

          {/* Internal Glowing Quantum Core */}
          <motion.div
            animate={
              reducedMotion
                ? { scale: 1, opacity: 0.85 }
                : { scale: [1, 1.08, 1], opacity: [0.85, 1, 0.85] }
            }
            transition={
              reducedMotion
                ? { duration: 0 }
                : {
                    duration: hasActiveRun ? 2 : 4,
                    repeat: Infinity,
                    ease: 'easeInOut',
                  }
            }
            className={`w-16 h-16 rounded-full bg-gradient-to-tr ${theme.innerCore} shadow-[0_0_24px_currentColor] flex flex-col items-center justify-center text-center`}
          >
            {/* Center AIQA Branding Typography */}
            <span className="text-white font-black text-xs tracking-widest drop-shadow-[0_2px_4px_rgba(0,0,0,0.8)] font-sans">
              AIQA
            </span>
            <span className="text-[7px] font-mono font-bold tracking-tighter text-white/90 drop-shadow-sm -mt-0.5">
              CORE
            </span>
          </motion.div>
        </div>

        {/* Ambient Particle Sparks */}
        <div className="absolute top-2 left-6 w-1 h-1 rounded-full bg-yellow-400 animate-ping opacity-60" />
        <div className="absolute bottom-4 right-6 w-1 h-1 rounded-full bg-violet-400 animate-pulse opacity-70" />
      </div>

      {/* Center Labels & Telemetry Subtitle */}
      <div className="relative z-10 text-center mt-1 space-y-1">
        <div className="flex items-center justify-center gap-1.5">
          <span className="text-xs font-bold tracking-wider text-slate-100 uppercase font-sans">
            AIQA Quality Core
          </span>
          <Sparkles className="w-3 h-3 text-yellow-400" />
        </div>
        <div className="text-[10px] font-mono text-slate-400">
          Autonomous Verification Engine • <span className="text-slate-300">{projectName}</span>
        </div>

        {/* Real-time Status Badge */}
        <div className="pt-1">
          <span
            className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold border ${theme.badgeBg}`}
          >
            <StatusIcon className="w-3 h-3" />
            <span>{theme.statusLabel}</span>
          </span>
        </div>
      </div>
    </div>
  );
};

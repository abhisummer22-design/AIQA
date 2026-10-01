/**
 * SectionHeader Component
 * Unified enterprise header for pages and modular panes
 */

import React from 'react';
import { LucideIcon } from 'lucide-react';

interface SectionHeaderProps {
  title: string;
  subtitle?: string;
  badge?: string;
  icon?: LucideIcon;
  actions?: React.ReactNode;
  className?: string;
}

export const SectionHeader: React.FC<SectionHeaderProps> = ({
  title,
  subtitle,
  badge,
  icon: Icon,
  actions,
  className = '',
}) => {
  return (
    <div className={`flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 mb-5 border-b border-border-default ${className}`}>
      <div className="flex items-center gap-2.5">
        {Icon && (
          <div className="w-8 h-8 rounded-md bg-surface-card border border-border-default flex items-center justify-center text-violet-400 shrink-0">
            <Icon className="w-4 h-4" />
          </div>
        )}
        <div>
          <div className="flex items-center gap-2">
            {/* Theme tokens, not the hardcoded dark-theme literals these used
                to carry. `text-slate-100` is #f1f5f9 — byte-for-byte the DARK
                theme's `--color-content-primary` — so on the light theme
                (`--color-surface-page: #f8fafc`) every page title rendered at
                ~1.05:1 contrast and was effectively invisible. The light theme
                is the default on a first visit from a light-themed OS
                (`ThemeContext`'s `systemPrefersLight()` fallback), so this was
                the first thing a new user saw. Tokens repaint from
                `index.css`'s single `[data-theme='light']` block. */}
            <h1 className="text-base md:text-lg font-bold tracking-tight text-content-primary font-sans">
              {title}
            </h1>
            {badge && (
              <span className="px-2 py-0.2 rounded text-[10px] font-mono font-semibold bg-surface-card text-violet-300 border border-violet-800/60 uppercase">
                {badge}
              </span>
            )}
          </div>
          {subtitle && (
            <p className="text-xs text-content-secondary mt-0.5 leading-normal">
              {subtitle}
            </p>
          )}
        </div>
      </div>

      {actions && <div className="flex items-center gap-2 self-start sm:self-auto shrink-0">{actions}</div>}
    </div>
  );
};

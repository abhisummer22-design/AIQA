/**
 * Global Command Palette (Cmd+K)
 * Fast navigation across the 8 AIQA workspaces and quick control plane operations
 *
 * FE-4: the "Workspaces" list below is generated from `routeConfig.tsx`
 * (the same source Sidebar/AppRoutes use) instead of a second hardcoded
 * path list, and navigation goes through the real router (`useNavigate`).
 */

import React, { useRef, useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Search, Plus, X, ArrowRight, Layers } from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { routeDefs } from '../../routes/routeConfig';
import { Modal } from '../common/Modal';

/** Whether to advertise the Mac glyph or the Windows/Linux one. `AIQAContext`
 *  binds the shortcut to the Mac meta key only on Apple platforms, so showing
 *  `⌘K` to a Windows user named a shortcut that does not open this palette. */
const IS_APPLE =
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad|iPod/.test(navigator.platform);

export const CommandPalette: React.FC = () => {
  const { isCommandPaletteOpen, setCommandPaletteOpen, setCurrentEnvironment } = useAIQA();
  const navigate = useNavigate();

  const [search, setSearch] = useState('');
  // `Modal` moves focus here on open, replacing the `autoFocus` attribute the
  // search field used to carry -- autoFocus alone does not survive being applied
  // to an element that mounts later in the tree.
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isCommandPaletteOpen) {
      setSearch('');
    }
  }, [isCommandPaletteOpen]);

  if (!isCommandPaletteOpen) return null;

  const actions = [
    {
      category: 'Workspaces',
      items: routeDefs.map((r) => ({
        id: r.id,
        label: `Go to ${r.label}`,
        icon: r.icon,
        action: () => navigate(r.path),
      })),
    },
    {
      category: 'Quick Actions',
      items: [
        {
          id: 'new-run',
          label: 'Launch New Execution Run...',
          icon: Plus,
          action: () => {
            navigate('/execution');
            setCommandPaletteOpen(false);
          },
        },
        {
          id: 'switch-env-demo',
          label: 'Switch Environment to DEMO',
          icon: Layers,
          action: () => setCurrentEnvironment('demo'),
        },
        {
          id: 'switch-env-uat',
          label: 'Switch Environment to UAT',
          icon: Layers,
          action: () => setCurrentEnvironment('uat'),
        },
        {
          id: 'switch-env-live',
          label: 'Switch Environment to LIVE',
          icon: Layers,
          action: () => setCurrentEnvironment('live'),
        },
      ],
    },
  ];

  const filtered = actions
    .map((group) => ({
      ...group,
      items: group.items.filter((item) =>
        item.label.toLowerCase().includes(search.toLowerCase())
      ),
    }))
    .filter((group) => group.items.length > 0);

  const handleSelect = (actionFn: () => void) => {
    actionFn();
    setCommandPaletteOpen(false);
  };

  return (
    // Migrated to the shared `Modal` primitive.
    //
    // This had no `role="dialog"`, no `aria-modal`, no `aria-label`, and --
    // the part that made its own UI a lie -- NO keydown handler of any kind,
    // while its own footer read "Press ESC to close". The backdrop had no
    // onClick either, so the only ways out were the (unnamed) X button or
    // activating a result. There was no focus restore either: on close focus fell
    // to <body> and the palette re-opened with nothing focused.
    <Modal
      id="command-palette"
      ariaLabel="Command palette"
      onClose={() => setCommandPaletteOpen(false)}
      initialFocusRef={searchRef}
      panelClassName="max-w-xl"
      overlayClassName="items-start justify-center pt-20"
    >
      <div className="flex flex-col overflow-hidden">
        {/* Search Input Bar */}
        <div className="flex items-center gap-3 px-4 py-2.5 border-b border-border-default bg-surface-page">
          <Search className="w-4 h-4 text-violet-400 shrink-0" />
          <label htmlFor="command-palette-search" className="sr-only">
            Search commands and workspaces
          </label>
          <input
            id="command-palette-search"
            ref={searchRef}
            type="text"
            placeholder="Type a command or search workspace..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="flex-1 bg-transparent border-none outline-none text-xs text-content-primary placeholder-content-muted font-sans"
          />
          <button
            type="button"
            onClick={() => setCommandPaletteOpen(false)}
            aria-label="Close command palette"
            className="p-1 text-content-muted hover:text-content-secondary rounded cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Results List */}
        <div className="max-h-80 overflow-y-auto p-2 space-y-2">
          {filtered.length === 0 ? (
            <div className="py-6 text-center text-xs text-content-muted font-mono">
              No matching commands found for "{search}"
            </div>
          ) : (
            filtered.map((group) => (
              <div key={group.category} className="space-y-0.5">
                <div className="px-2 py-1 text-[10px] font-mono uppercase tracking-wider text-content-muted font-semibold">
                  {group.category}
                </div>
                {group.items.map((item) => {
                  const Icon = item.icon;
                  return (
                    <button
                      key={item.id}
                      type="button"
                      onClick={() => handleSelect(item.action)}
                      className="w-full flex items-center justify-between px-3 py-1.5 rounded-md text-xs text-content-secondary hover:text-white hover:bg-violet-600 transition-colors group cursor-pointer"
                    >
                      <div className="flex items-center gap-2.5">
                        <Icon className="w-3.5 h-3.5 text-content-muted group-hover:text-white" />
                        <span>{item.label}</span>
                      </div>
                      <ArrowRight className="w-3.5 h-3.5 text-content-muted group-hover:text-white opacity-0 group-hover:opacity-100 transition-opacity" />
                    </button>
                  );
                })}
              </div>
            ))
          )}
        </div>

        {/* Footer shortcuts. `AIQAContext` binds ⌘K on the Mac meta key and Ctrl+K
            elsewhere, and this app ships for Windows first -- so the glyph is
            resolved from the platform rather than hardcoded to the Mac one it
            previously advertised on every machine. */}
        <div className="px-4 py-2 bg-surface-page border-t border-border-default text-[10px] font-mono text-content-muted flex justify-between">
          <span>Press ESC to close</span>
          <span>Use {IS_APPLE ? '⌘' : 'Ctrl+'}K anytime to open</span>
        </div>
      </div>
    </Modal>
  );
};

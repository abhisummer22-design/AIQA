/**
 * Phase 7: Quality Graph Section
 * Interactive visual representation of quality nodes and relational edges with node inspector drawer.
 */

import React, { useState, useMemo } from 'react';
import { Network, Filter, Search, X, Layers, Box, Cpu, AlertTriangle, Wrench, Shield } from 'lucide-react';
import type { QualityGraphResponse, QualityNodeRow } from '../../types/contract';

interface QualityGraphSectionProps {
  graph: QualityGraphResponse | null;
  loading?: boolean;
}

export const QualityGraphSection: React.FC<QualityGraphSectionProps> = ({ graph, loading }) => {
  const [selectedNode, setSelectedNode] = useState<QualityNodeRow | null>(null);
  const [typeFilter, setTypeFilter] = useState('ALL');
  const [search, setSearch] = useState('');

  const nodes = graph?.nodes ?? [];
  const relations = graph?.relations ?? [];

  // Distinct node types
  const nodeTypes = useMemo(() => {
    const types = new Set<string>();
    nodes.forEach((n) => types.add(n.node_type));
    return Array.from(types);
  }, [nodes]);

  // Filtered nodes
  const filteredNodes = useMemo(() => {
    return nodes.filter((n) => {
      if (typeFilter !== 'ALL' && n.node_type !== typeFilter) return false;
      const q = search.trim().toLowerCase();
      if (q && !n.name.toLowerCase().includes(q) && !n.id.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [nodes, typeFilter, search]);

  // Connected relations for the selected node
  const connectedRelations = useMemo(() => {
    if (!selectedNode) return [];
    return relations.filter(
      (r) => r.from_node_id === selectedNode.id || r.to_node_id === selectedNode.id
    );
  }, [selectedNode, relations]);

  const getNodeIcon = (type: string) => {
    switch (type.toLowerCase()) {
      case 'test':
        return <Box className="w-3.5 h-3.5 text-emerald-400" />;
      case 'task':
        return <Cpu className="w-3.5 h-3.5 text-sky-400" />;
      case 'failure_cluster':
        return <AlertTriangle className="w-3.5 h-3.5 text-rose-400" />;
      case 'healing_strategy':
        return <Wrench className="w-3.5 h-3.5 text-indigo-400" />;
      default:
        return <Layers className="w-3.5 h-3.5 text-violet-400" />;
    }
  };

  const getNodeBadgeColor = (type: string) => {
    switch (type.toLowerCase()) {
      case 'test':
        return 'bg-emerald-950/40 text-emerald-400 border-emerald-800/40';
      case 'task':
        return 'bg-sky-950/40 text-sky-400 border-sky-800/40';
      case 'failure_cluster':
        return 'bg-rose-950/40 text-rose-400 border-rose-800/40';
      case 'healing_strategy':
        return 'bg-indigo-950/40 text-indigo-400 border-indigo-800/40';
      default:
        return 'bg-violet-950/40 text-violet-400 border-violet-800/40';
    }
  };

  if (loading) {
    return (
      <div className="bg-slate-900/40 border border-border-default rounded-xl p-5 animate-pulse" data-testid="graph-loading">
        <div className="h-6 w-48 bg-slate-800 rounded mb-4" />
        <div className="h-64 bg-slate-800/40 rounded" />
      </div>
    );
  }

  return (
    <div className="bg-slate-900/40 border border-border-default rounded-xl p-5" data-testid="quality-graph-section">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 mb-4">
        <div>
          <h3 className="text-sm font-semibold text-content-primary flex items-center gap-2">
            <Network className="w-4 h-4 text-violet-400" />
            <span>Quality Graph Knowledge Network</span>
            <span className="text-xs px-2 py-0.5 rounded-full bg-slate-800 text-content-secondary font-mono">
              {nodes.length} nodes · {relations.length} edges
            </span>
          </h3>
          <p className="text-xs text-content-secondary mt-0.5">
            Relational topology connecting tests, runs, clusters, and healing strategies.
          </p>
        </div>

        {/* Filter controls */}
        <div className="flex flex-wrap items-center gap-2">
          <div className="relative min-w-[160px]">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-content-secondary" />
            <input
              type="text"
              placeholder="Search nodes..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="w-full pl-8 pr-3 py-1 text-xs bg-slate-950 border border-border-default rounded-md text-content-primary placeholder-slate-500 focus:outline-none focus:border-violet-500"
              data-testid="graph-search-input"
            />
          </div>

          <select
            value={typeFilter}
            onChange={(e) => {
              setTypeFilter(e.target.value);
              setSelectedNode(null);
            }}
            className="text-xs bg-slate-950 border border-border-default rounded-md px-2 py-1 text-content-secondary focus:outline-none focus:border-violet-500"
            data-testid="graph-type-filter"
          >
            <option value="ALL">All Types</option>
            {nodeTypes.map((type) => (
              <option key={type} value={type}>
                {type}
              </option>
            ))}
          </select>
        </div>
      </div>

      {nodes.length === 0 ? (
        <div className="py-16 text-center text-content-secondary text-xs border border-dashed border-border-default/80 rounded-lg" data-testid="graph-empty">
          <Network className="w-8 h-8 text-content-muted mx-auto mb-2" />
          No Quality Graph entities populated yet for this project. Execute runs to construct relational topology.
        </div>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
          {/* Graph Nodes Canvas / Grid */}
          <div className="lg:col-span-2 bg-slate-950/60 border border-border-default/80 rounded-xl p-4 min-h-[360px] flex flex-col justify-between">
            <div className="text-[11px] font-mono text-content-muted mb-3 flex items-center justify-between">
              <span>Interactive Graph Entities ({filteredNodes.length})</span>
              <span>Click node to inspect relations</span>
            </div>

            <div className="grid grid-cols-2 sm:grid-cols-3 gap-2.5 max-h-[380px] overflow-y-auto p-1">
              {filteredNodes.map((node) => {
                const isSelected = selectedNode?.id === node.id;
                return (
                  <button
                    key={node.id}
                    onClick={() => setSelectedNode(node)}
                    className={`p-3 rounded-lg border text-left transition-all flex flex-col justify-between ${
                      isSelected
                        ? 'bg-violet-950/40 border-violet-500 ring-1 ring-violet-500/50 shadow-md shadow-violet-950'
                        : 'bg-slate-900/60 border-border-default hover:border-slate-700'
                    }`}
                    data-testid={`graph-node-${node.id}`}
                  >
                    <div className="flex items-center justify-between mb-1.5">
                      <span
                        className={`text-[9px] font-mono px-1.5 py-0.5 rounded border ${getNodeBadgeColor(
                          node.node_type
                        )}`}
                      >
                        {node.node_type}
                      </span>
                      {getNodeIcon(node.node_type)}
                    </div>
                    <div className="font-semibold text-xs text-content-primary truncate w-full" title={node.name}>
                      {node.name}
                    </div>
                    <div className="text-[10px] font-mono text-content-muted truncate w-full mt-1">
                      {node.id}
                    </div>
                  </button>
                );
              })}
            </div>

            {/* Edge summary footer */}
            <div className="mt-3 pt-3 border-t border-border-default/80 text-[11px] font-mono text-content-secondary flex items-center justify-between">
              <span>Topology: {nodes.length} Vertices</span>
              <span>Connectivity: {relations.length} Directed Edges</span>
            </div>
          </div>

          {/* Node Inspector Drawer */}
          <div className="bg-slate-950/60 border border-border-default/80 rounded-xl p-4" data-testid="graph-node-inspector">
            {selectedNode ? (
              <div>
                <div className="flex items-center justify-between border-b border-border-default pb-3 mb-3">
                  <div className="flex items-center gap-2">
                    {getNodeIcon(selectedNode.node_type)}
                    <div>
                      <h4 className="text-xs font-bold text-content-primary truncate max-w-[180px]">
                        {selectedNode.name}
                      </h4>
                      <span className="text-[10px] font-mono text-content-secondary">{selectedNode.node_type}</span>
                    </div>
                  </div>
                  <button
                    onClick={() => setSelectedNode(null)}
                    className="p-1 rounded text-content-secondary hover:text-content-primary"
                    title="Close Inspector"
                  >
                    <X className="w-4 h-4" />
                  </button>
                </div>

                <div className="space-y-3 text-xs">
                  <div>
                    <span className="text-[10px] text-content-muted uppercase block font-mono">Node ID</span>
                    <span className="font-mono text-[11px] text-content-secondary break-all">{selectedNode.id}</span>
                  </div>

                  {selectedNode.external_ref && (
                    <div>
                      <span className="text-[10px] text-content-muted uppercase block font-mono">External Ref</span>
                      <span className="font-mono text-[11px] text-content-secondary break-all">
                        {selectedNode.external_ref}
                      </span>
                    </div>
                  )}

                  {selectedNode.metadata && (
                    <div>
                      <span className="text-[10px] text-content-muted uppercase block font-mono">Metadata</span>
                      <pre className="bg-slate-900/80 p-2 rounded text-[10px] font-mono text-content-secondary overflow-x-auto max-h-28">
                        {selectedNode.metadata}
                      </pre>
                    </div>
                  )}

                  {/* Connected Relations */}
                  <div className="pt-2 border-t border-border-default">
                    <span className="text-[10px] text-content-secondary uppercase block font-mono mb-1.5">
                      Connected Edges ({connectedRelations.length})
                    </span>
                    {connectedRelations.length === 0 ? (
                      <div className="text-[11px] text-content-muted italic">No direct edges linked.</div>
                    ) : (
                      <div className="space-y-1.5 max-h-36 overflow-y-auto pr-1">
                        {connectedRelations.map((rel) => {
                          const isOutgoing = rel.from_node_id === selectedNode.id;
                          return (
                            <div
                              key={rel.id}
                              className="bg-slate-900/60 border border-border-default p-2 rounded text-[10px] font-mono"
                            >
                              <div className="text-violet-300 font-bold mb-0.5">
                                {isOutgoing ? '→ OUTGOING' : '← INCOMING'}: {rel.relation_type}
                              </div>
                              <div className="text-content-secondary truncate">
                                Target: {isOutgoing ? rel.to_node_id : rel.from_node_id}
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    )}
                  </div>
                </div>
              </div>
            ) : (
              <div className="h-full flex flex-col items-center justify-center text-center py-16 text-content-muted text-xs">
                <Box className="w-8 h-8 opacity-40 mb-2" />
                Select any node from the graph to inspect properties and relational edges.
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
};

import React, { useState, useRef, useEffect, useMemo } from "react";
import {
  Search,
  ChevronDown,
  Cpu,
  Globe,
  Server,
  Zap,
  Check,
  Sparkles,
  Shield,
  Clock,
  Layers,
  ArrowRight,
  HardDrive,
  Filter,
  Plus,
  ExternalLink
} from "lucide-react";
import { LocalOllamaModel, CloudApiProvider, ModelEndpointItem, BrainEngineSource } from "../../types";

interface ModelEndpointSearchDropdownProps {
  localModels: LocalOllamaModel[];
  cloudProviders: CloudApiProvider[];
  ollamaBaseUrl?: string;
  selectedModelTag?: string;
  selectedEngine?: string;
  onSelect: (item: ModelEndpointItem) => void;
  label?: string;
  variant?: "default" | "compact" | "modelA" | "modelB";
  onSetAsPrimary?: (modelId: string) => void;
  className?: string;
  placeholder?: string;
}

export const ModelEndpointSearchDropdown: React.FC<ModelEndpointSearchDropdownProps> = ({
  localModels,
  cloudProviders,
  ollamaBaseUrl = "http://localhost:11434",
  selectedModelTag,
  selectedEngine,
  onSelect,
  label,
  variant = "default",
  onSetAsPrimary,
  className = "",
  placeholder = "Search local Ollama instances or cloud APIs..."
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [activeFilter, setActiveFilter] = useState<"all" | "local" | "cloud" | "custom">("all");
  const containerRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Close dropdown on click outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  // Focus input on open
  useEffect(() => {
    if (isOpen && searchInputRef.current) {
      setTimeout(() => searchInputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  // Aggregate and normalize all models and endpoints into a unified searchable list
  const allEndpoints: ModelEndpointItem[] = useMemo(() => {
    const items: ModelEndpointItem[] = [];

    // 1. Local Ollama Models (CUDA Node)
    localModels.forEach((m) => {
      items.push({
        id: `ollama-${m.id}`,
        name: m.name,
        engine: "ollama",
        modelTag: m.id,
        endpointUrl: ollamaBaseUrl,
        instanceType: "local_ollama",
        instanceLabel: `Local Ollama (${ollamaBaseUrl})`,
        parameters: m.parameters,
        quantization: m.quantization,
        contextWindow: `${Math.round(m.contextLength / 1024)}k tokens`,
        latencyMs: m.isPrimary ? 14 : 22,
        tokensPerSec: m.id.includes("70b") ? 38 : 94,
        costPer1k: "$0.0000 (Free Local)",
        status: m.isPrimary ? "loaded" : "available",
        isPrimary: m.isPrimary,
        specialization: m.specialization,
        providerName: "Ollama CUDA Engine"
      });
    });

    // Secondary LAN Ollama instance preset for quick switching
    items.push({
      id: "ollama-remote-qwen72b",
      name: "Qwen 2.5 72B Instruct (LAN Cluster)",
      engine: "ollama",
      modelTag: "qwen2.5:72b-instruct-q4_K_M",
      endpointUrl: "http://192.168.1.50:11434",
      instanceType: "local_ollama",
      instanceLabel: "Remote Ollama (192.168.1.50:11434)",
      parameters: "72B",
      quantization: "Q4_K_M",
      contextWindow: "128k tokens",
      latencyMs: 48,
      tokensPerSec: 42,
      costPer1k: "$0.0000 (LAN Cluster)",
      status: "available",
      isPrimary: false,
      specialization: "Heavy Enterprise Architecture, Deep Analysis",
      providerName: "Ollama LAN Node 2"
    });

    // 2. Cloud Providers & Frontier APIs
    cloudProviders.forEach((p) => {
      p.availableModels.forEach((mod) => {
        let cost = p.costPer1kTokens;
        let latency = p.latencyMs || 220;
        let tps = p.tokensPerSec || 110;
        let params = "Frontier";

        if (mod.includes("flash")) {
          latency = 160;
          tps = 155;
          params = "Pro/Flash";
        } else if (mod.includes("pro") || mod.includes("gpt-4o")) {
          latency = 290;
          tps = 95;
          params = "High Reasoning";
        } else if (mod.includes("groq") || p.id === "groq") {
          latency = 85;
          tps = 380;
          params = "70B / LPU";
        } else if (mod.includes("deepseek")) {
          latency = 480;
          tps = 65;
          params = "671B MoE";
        }

        const isCustom = p.id === "custom_openai";

        items.push({
          id: `${p.id}-${mod}`,
          name: `${p.name} - ${mod}`,
          engine: p.id,
          modelTag: mod,
          endpointUrl: p.customBaseUrl || (p.id === "gemini" ? "generativelanguage.googleapis.com" : "api.openai.com/v1"),
          instanceType: isCustom ? "custom_endpoint" : "remote_api",
          instanceLabel: isCustom ? "Private Custom Ingress" : `${p.provider}`,
          parameters: params,
          quantization: "FP16 / Server",
          contextWindow: p.contextWindow,
          latencyMs: latency,
          tokensPerSec: tps,
          costPer1k: cost,
          status: p.status === "invalid_key" ? "offline" : p.hasKey ? "ready" : "testing",
          isPrimary: false,
          specialization: p.features.join(" • ") || p.description,
          providerName: p.name
        });
      });
    });

    return items;
  }, [localModels, cloudProviders, ollamaBaseUrl]);

  // Determine current active selection
  const currentItem = useMemo(() => {
    if (selectedModelTag) {
      const match = allEndpoints.find(
        (e) =>
          (e.modelTag === selectedModelTag || e.name.toLowerCase().includes(selectedModelTag.toLowerCase())) &&
          (!selectedEngine || e.engine === selectedEngine)
      );
      if (match) return match;
    }
    if (selectedEngine) {
      const matchEngine = allEndpoints.find((e) => e.engine === selectedEngine);
      if (matchEngine) return matchEngine;
    }
    return allEndpoints[0];
  }, [allEndpoints, selectedModelTag, selectedEngine]);

  // Filter items by search text & category tab
  const filteredEndpoints = useMemo(() => {
    const q = searchQuery.toLowerCase().trim();
    return allEndpoints.filter((item) => {
      // Category filter
      if (activeFilter === "local" && item.instanceType !== "local_ollama") return false;
      if (activeFilter === "cloud" && item.instanceType !== "remote_api") return false;
      if (activeFilter === "custom" && item.instanceType !== "custom_endpoint") return false;

      // Text query match
      if (!q) return true;
      return (
        item.name.toLowerCase().includes(q) ||
        item.modelTag.toLowerCase().includes(q) ||
        item.engine.toLowerCase().includes(q) ||
        item.providerName.toLowerCase().includes(q) ||
        item.parameters.toLowerCase().includes(q) ||
        item.quantization.toLowerCase().includes(q) ||
        item.specialization.toLowerCase().includes(q) ||
        item.endpointUrl.toLowerCase().includes(q)
      );
    });
  }, [allEndpoints, searchQuery, activeFilter]);

  const getVariantStyles = () => {
    if (variant === "modelA") {
      return {
        border: "border-indigo-500/60 hover:border-indigo-400",
        bg: "bg-indigo-950/30",
        badge: "bg-indigo-500/20 text-indigo-300 border-indigo-500/30",
        tag: "MODEL A"
      };
    }
    if (variant === "modelB") {
      return {
        border: "border-cyan-500/60 hover:border-cyan-400",
        bg: "bg-cyan-950/30",
        badge: "bg-cyan-500/20 text-cyan-300 border-cyan-500/30",
        tag: "MODEL B"
      };
    }
    if (variant === "compact") {
      return {
        border: "border-zinc-700 hover:border-zinc-500",
        bg: "bg-zinc-900/80",
        badge: "bg-zinc-800 text-slate-300 border-zinc-700",
        tag: "ACTIVE"
      };
    }
    return {
      border: "border-zinc-800 hover:border-zinc-600",
      bg: "bg-zinc-900/90",
      badge: "bg-indigo-500/10 text-indigo-300 border-indigo-500/30",
      tag: "INFERENCE"
    };
  };

  const styles = getVariantStyles();

  const handleSelect = (item: ModelEndpointItem) => {
    onSelect(item);
    setIsOpen(false);
  };

  return (
    <div ref={containerRef} className={`relative font-mono ${className}`}>
      {label && (
        <div className="flex items-center justify-between mb-1.5">
          <label className="text-[11px] font-bold text-slate-300 uppercase tracking-wider flex items-center gap-1.5">
            {variant === "modelA" && <span className="w-2 h-2 rounded-full bg-indigo-400 animate-pulse" />}
            {variant === "modelB" && <span className="w-2 h-2 rounded-full bg-cyan-400 animate-pulse" />}
            <span>{label}</span>
          </label>
          <span className="text-[10px] text-slate-500">
            {allEndpoints.length} models/endpoints available
          </span>
        </div>
      )}

      {/* Dropdown Trigger Button */}
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className={`w-full p-3 rounded-xl border ${styles.border} ${styles.bg} text-left transition-all duration-150 flex items-center justify-between gap-3 shadow-md focus:outline-none focus:ring-1 focus:ring-indigo-500/40`}
      >
        <div className="flex items-center gap-2.5 min-w-0">
          <div
            className={`w-8 h-8 rounded-lg flex items-center justify-center shrink-0 ${
              currentItem?.instanceType === "local_ollama"
                ? "bg-indigo-500/20 text-indigo-300 border border-indigo-500/40"
                : currentItem?.instanceType === "custom_endpoint"
                ? "bg-purple-500/20 text-purple-300 border border-purple-500/40"
                : "bg-cyan-500/20 text-cyan-300 border border-cyan-500/40"
            }`}
          >
            {currentItem?.instanceType === "local_ollama" ? (
              <Cpu className="w-4 h-4" />
            ) : currentItem?.instanceType === "custom_endpoint" ? (
              <Server className="w-4 h-4" />
            ) : (
              <Globe className="w-4 h-4" />
            )}
          </div>

          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-xs font-bold text-white truncate">
                {currentItem?.name || "Select Model or API Endpoint"}
              </span>
              {currentItem?.isPrimary && (
                <span className="text-[9px] px-1.5 py-0.2 rounded font-bold bg-amber-500/20 text-amber-300 border border-amber-500/30">
                  PRIMARY
                </span>
              )}
            </div>
            <div className="flex items-center gap-2 text-[10px] text-slate-400 truncate mt-0.5">
              <span className="truncate">{currentItem?.instanceLabel}</span>
              <span>•</span>
              <span className="text-slate-300 font-semibold">{currentItem?.parameters}</span>
              <span>•</span>
              <span className="text-emerald-400 font-bold">{currentItem?.latencyMs}ms</span>
              <span>•</span>
              <span className="text-indigo-300">{currentItem?.tokensPerSec} t/s</span>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          <span className={`text-[9px] font-bold px-2 py-0.5 rounded border ${styles.badge}`}>
            {currentItem?.instanceType === "local_ollama" ? "LOCAL" : "CLOUD"}
          </span>
          <ChevronDown
            className={`w-4 h-4 text-slate-400 transition-transform duration-200 ${
              isOpen ? "rotate-180 text-white" : ""
            }`}
          />
        </div>
      </button>

      {/* Searchable Dropdown Popup Menu */}
      {isOpen && (
        <div className="absolute left-0 right-0 top-full mt-2 z-50 rounded-2xl bg-zinc-950/95 border border-zinc-700/80 shadow-2xl backdrop-blur-xl overflow-hidden animate-in fade-in zoom-in-95 duration-150">
          {/* Header Search Input */}
          <div className="p-3 border-b border-zinc-800/80 bg-zinc-900/60">
            <div className="relative">
              <Search className="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2" />
              <input
                ref={searchInputRef}
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={placeholder}
                className="w-full pl-9 pr-3 py-2 rounded-xl bg-zinc-950 border border-zinc-800 text-xs text-white placeholder:text-slate-500 focus:outline-none focus:border-indigo-500"
              />
              {searchQuery && (
                <button
                  type="button"
                  onClick={() => setSearchQuery("")}
                  className="absolute right-2.5 top-1/2 -translate-y-1/2 text-[10px] text-slate-400 hover:text-white px-1.5 py-0.5 rounded bg-zinc-800"
                >
                  Clear
                </button>
              )}
            </div>

            {/* Quick Category Filter Tabs */}
            <div className="flex items-center gap-1.5 mt-2.5 overflow-x-auto pb-0.5">
              {[
                { id: "all", label: `All (${allEndpoints.length})` },
                {
                  id: "local",
                  label: `Local Ollama (${allEndpoints.filter((e) => e.instanceType === "local_ollama").length})`
                },
                {
                  id: "cloud",
                  label: `Cloud Frontier (${allEndpoints.filter((e) => e.instanceType === "remote_api").length})`
                },
                {
                  id: "custom",
                  label: `Custom Ingress (${allEndpoints.filter((e) => e.instanceType === "custom_endpoint").length})`
                }
              ].map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setActiveFilter(tab.id as any)}
                  className={`px-2.5 py-1 rounded-lg text-[10px] font-bold whitespace-nowrap transition-colors ${
                    activeFilter === tab.id
                      ? "bg-indigo-600 text-white"
                      : "bg-zinc-900 text-slate-400 hover:bg-zinc-800 hover:text-slate-200 border border-zinc-800"
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </div>
          </div>

          {/* List of Models & Endpoints */}
          <div className="max-h-72 overflow-y-auto divide-y divide-zinc-900/80 p-1.5">
            {filteredEndpoints.length === 0 ? (
              <div className="p-8 text-center text-slate-500 text-xs">
                No matching local instances or remote API models found for "{searchQuery}"
              </div>
            ) : (
              filteredEndpoints.map((item) => {
                const isSelected =
                  currentItem?.id === item.id ||
                  (item.modelTag === selectedModelTag && item.engine === selectedEngine);

                return (
                  <div
                    key={item.id}
                    className={`p-3 rounded-xl transition-all group cursor-pointer ${
                      isSelected
                        ? "bg-indigo-950/40 border border-indigo-500/40"
                        : "hover:bg-zinc-900/80 border border-transparent"
                    }`}
                    onClick={() => handleSelect(item)}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="flex items-start gap-2.5 min-w-0">
                        <div
                          className={`w-7 h-7 rounded-lg flex items-center justify-center shrink-0 mt-0.5 ${
                            item.instanceType === "local_ollama"
                              ? "bg-indigo-500/20 text-indigo-300"
                              : item.instanceType === "custom_endpoint"
                              ? "bg-purple-500/20 text-purple-300"
                              : "bg-cyan-500/20 text-cyan-300"
                          }`}
                        >
                          {item.instanceType === "local_ollama" ? (
                            <Cpu className="w-3.5 h-3.5" />
                          ) : item.instanceType === "custom_endpoint" ? (
                            <Server className="w-3.5 h-3.5" />
                          ) : (
                            <Globe className="w-3.5 h-3.5" />
                          )}
                        </div>

                        <div className="min-w-0">
                          <div className="flex items-center gap-2 flex-wrap">
                            <span className="text-xs font-bold text-white group-hover:text-indigo-300 transition-colors">
                              {item.name}
                            </span>
                            <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-zinc-800 text-slate-400 border border-zinc-700">
                              {item.parameters}
                            </span>
                            {item.quantization && item.quantization !== "FP16 / Server" && (
                              <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-zinc-800/80 text-indigo-300">
                                {item.quantization}
                              </span>
                            )}
                            {item.isPrimary && (
                              <span className="text-[9px] font-mono font-bold px-1.5 py-0.2 rounded bg-amber-500/20 text-amber-300 border border-amber-500/40">
                                PRIMARY
                              </span>
                            )}
                          </div>

                          <div className="text-[10px] text-slate-400 mt-1 line-clamp-1">
                            {item.specialization}
                          </div>

                          <div className="flex items-center gap-3 text-[10px] text-slate-500 mt-1.5 flex-wrap">
                            <span className="text-slate-400 font-medium">{item.instanceLabel}</span>
                            <span>•</span>
                            <span className="text-emerald-400 font-bold flex items-center gap-1">
                              <Clock className="w-3 h-3" />
                              {item.latencyMs}ms
                            </span>
                            <span>•</span>
                            <span className="text-indigo-300 font-bold flex items-center gap-1">
                              <Zap className="w-3 h-3" />
                              {item.tokensPerSec} t/s
                            </span>
                            <span>•</span>
                            <span className="text-slate-400">{item.costPer1k}</span>
                          </div>
                        </div>
                      </div>

                      <div className="flex items-center gap-2 shrink-0">
                        {isSelected ? (
                          <div className="w-6 h-6 rounded-full bg-indigo-600 text-white flex items-center justify-center">
                            <Check className="w-3.5 h-3.5" />
                          </div>
                        ) : (
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleSelect(item);
                            }}
                            className="px-2.5 py-1 rounded-lg bg-zinc-800 hover:bg-indigo-600 text-slate-300 hover:text-white text-[10px] font-bold transition-colors opacity-0 group-hover:opacity-100"
                          >
                            Switch &rarr;
                          </button>
                        )}
                      </div>
                    </div>
                  </div>
                );
              })
            )}
          </div>

          {/* Footer Info & Quick Action */}
          <div className="p-2.5 border-t border-zinc-800/80 bg-zinc-900/60 flex items-center justify-between text-[10px] text-slate-400">
            <span className="flex items-center gap-1.5">
              <Shield className="w-3.5 h-3.5 text-emerald-400" />
              <span>Zero-trust cryptographic model verification active</span>
            </span>
            <span className="text-slate-500 font-mono">
              Press ESC or click outside to dismiss
            </span>
          </div>
        </div>
      )}
    </div>
  );
};

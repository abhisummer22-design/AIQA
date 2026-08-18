/**
 * Model Gateway & AI Telemetry View (Contract §22)
 * Manages model routing, provider health, latency, token costs, and fallbacks.
 */

import React from 'react';
import {
  Brain,
  Cpu,
  Activity,
  Zap,
  Sliders,
  DollarSign,
  AlertTriangle,
  Radio,
} from 'lucide-react';
import { SectionHeader } from '../../components/common/SectionHeader';
import { BackendNotice } from '../../components/common/BackendNotice';
import { Card } from '../../components/common/Card';
import { MetricCard } from '../../components/common/MetricCard';

export function ModelGatewayView() {
  return (
    <div className="space-y-4">
      <SectionHeader
        title="Model Gateway & AI Telemetry"
        subtitle="Multi-provider LLM routing, latency monitoring, cost attribution, and automatic fallback health."
        badge="STAGED SPECIFICATION"
        badgeTone="warn"
      />

      <BackendNotice
        feature="AI Model Gateway & Telemetry Router"
        contractSection="§22 (Model Gateway APIs)"
        endpoint="GET /api/v1/models/health | GET /api/v1/models/telemetry"
        requires="Centralized model gateway in the backend. (Contract §22 strictly forbids the frontend from calling model providers directly with client-side API keys)."
        payloadExample={`// GET /api/v1/models/health
{
  "active_providers": [
    { "provider": "google-gemini", "model": "gemini-2.0-flash", "status": "ONLINE", "p95_latency_ms": 420 },
    { "provider": "anthropic-claude", "model": "claude-3-5-sonnet", "status": "STANDBY", "p95_latency_ms": 680 }
  ]
}`}
      />

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard label="Active Model Endpoints" value="—" subtext="BACKEND API REQUIRED" icon={<Brain size={14} />} />
        <MetricCard label="Inference Latency (p95)" value="—" subtext="BACKEND API REQUIRED" icon={<Activity size={14} />} />
        <MetricCard label="Token Consumption" value="—" subtext="BACKEND API REQUIRED" icon={<Zap size={14} />} />
        <MetricCard label="Automatic Fallback Events" value="—" subtext="BACKEND API REQUIRED" icon={<AlertTriangle size={14} />} />
      </div>

      <Card icon={<Brain size={14} />} title="Provider Routing & Health Monitor">
        <div className="py-10 px-4 text-center space-y-2">
          <div className="text-xs text-slate-500 font-mono">
            No backend model gateway configured. Staging state ready.
          </div>
          <p className="text-[11px] text-slate-600 max-w-md mx-auto">
            All model calls MUST route through the backend gateway to enforce prompt governance, token limits, and secure secret handling (Contract §22).
          </p>
        </div>
      </Card>
    </div>
  );
}

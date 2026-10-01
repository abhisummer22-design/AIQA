/**
 * Truthfulness Cleanup — Step 1 (Application Source connectivity).
 *
 * Previously, `handleConnect` performed only client-side URL-syntax
 * validation, waited a fixed 600ms, then unconditionally reported
 * `CONNECTED` with a fabricated `httpStatus: 200`, `sslValid: true`, and a
 * `Math.random()`-based fake ping time — for *any* syntactically valid URL,
 * including unreachable or nonexistent hosts. This was flagged CRITICAL by
 * the Phase 1-6 Final Consolidated Audit (Gap 1): no real network call ever
 * occurred, and nothing in the UI disclosed that.
 *
 * This version calls a real backend endpoint
 * (`aiqaApi.appSource.verify` -> `POST .../verify-application-source` ->
 * `app_source.rs`) that performs a real, bounded, SSRF-guarded outbound
 * request and returns a real, honest result. The UI now has seven genuinely
 * distinct states (`IDLE`/`CHECKING`/`CONNECTED`/`UNREACHABLE`/`REJECTED`/
 * `INVALID_URL`/`ERROR`) driven entirely by that real response — never a
 * fabricated status, never a randomized metric. See
 * `AIQA_STEP_1_APPLICATION_SOURCE_TRUTHFUL_CONNECTION_REPORT.md` for the
 * full before/after.
 */
import React, { useState } from 'react';
import {
  Globe,
  Figma,
  CheckCircle2,
  XCircle,
  AlertTriangle,
  ShieldOff,
  RefreshCw,
  ExternalLink,
  Shield,
  Server,
  Radio,
  Unlink,
  Link2,
} from 'lucide-react';
import { AppSourceType, AppSourceConnection, AppConnectionStatus } from '../../types';
import { ApiError, aiqaApi } from '../../services/aiqaApi';

interface ApplicationSourceSectionProps {
  appSources: Record<AppSourceType, AppSourceConnection>;
  onAppSourcesChange: (sources: Record<AppSourceType, AppSourceConnection>) => void;
  /** The real, currently-selected project id — required to call the real
   *  verification endpoint (project-scoped, same as every other
   *  real-outbound-action call in this app). `null` disables "Connect"
   *  with a truthful reason, rather than silently failing or faking a
   *  result. */
  projectId: string | null;
  disabled?: boolean;
}

interface SourceCardMeta {
  type: AppSourceType;
  label: string;
  subLabel: string;
  roleDescription: string;
  placeholder: string;
  icon: React.ComponentType<{ className?: string }>;
  accentColor: string;
  envBadge: string;
}

const SOURCES_META: SourceCardMeta[] = [
  {
    type: 'STAGING',
    label: 'Staging',
    subLabel: 'Development validation',
    roleDescription: 'Active branch builds, ephemeral features, and pre-release validation.',
    placeholder: 'https://staging.example.com',
    icon: Server,
    accentColor: 'text-yellow-400',
    envBadge: 'bg-yellow-950/80 border-yellow-700/80 text-yellow-300',
  },
  {
    type: 'DEMO',
    label: 'Demo',
    subLabel: 'Demonstration environment',
    roleDescription: 'Seeded staging environment with synthetic customer test datasets.',
    placeholder: 'https://demo.example.com',
    icon: Globe,
    accentColor: 'text-violet-400',
    envBadge: 'bg-violet-950/80 border-violet-700/80 text-violet-300',
  },
  {
    type: 'LIVE',
    label: 'Live',
    subLabel: 'Production environment',
    roleDescription: 'Live customer-facing environment for read-only synthetic regression.',
    placeholder: 'https://example.com',
    icon: Shield,
    accentColor: 'text-emerald-400',
    envBadge: 'bg-emerald-950/80 border-emerald-700/80 text-emerald-300',
  },
  {
    type: 'FIGMA',
    label: 'Figma',
    subLabel: 'Design Source',
    roleDescription: 'Inspect design tokens, component hierarchies, and interactive states.',
    placeholder: 'Paste Figma file/design URL',
    icon: Figma,
    accentColor: 'text-purple-400',
    envBadge: 'bg-purple-950/80 border-purple-700/80 text-purple-300',
  },
];

export const ApplicationSourceSection: React.FC<ApplicationSourceSectionProps> = ({
  appSources,
  onAppSourcesChange,
  projectId,
  disabled = false,
}) => {
  const [inputUrls, setInputUrls] = useState<Record<AppSourceType, string>>({
    STAGING: appSources.STAGING?.url || '',
    DEMO: appSources.DEMO?.url || '',
    LIVE: appSources.LIVE?.url || '',
    FIGMA: appSources.FIGMA?.url || '',
  });

  const [validationErrors, setValidationErrors] = useState<Record<AppSourceType, string | null>>({
    STAGING: null,
    DEMO: null,
    LIVE: null,
    FIGMA: null,
  });

  const getStatusBadge = (status: AppConnectionStatus) => {
    switch (status) {
      case 'CONNECTED':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-emerald-950/80 border border-emerald-700 text-emerald-300">
            <CheckCircle2 className="w-3 h-3 text-emerald-400" />
            <span>Connected</span>
          </span>
        );
      case 'CHECKING':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-yellow-950/80 border border-yellow-700 text-yellow-300">
            <RefreshCw className="w-3 h-3 text-yellow-400 animate-spin" />
            <span>Checking</span>
          </span>
        );
      case 'INVALID_URL':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-amber-950/80 border border-amber-700 text-amber-300">
            <AlertTriangle className="w-3 h-3 text-amber-400" />
            <span>Invalid URL</span>
          </span>
        );
      case 'UNREACHABLE':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-rose-950/80 border border-rose-700 text-rose-300">
            <XCircle className="w-3 h-3 text-rose-400" />
            <span>Unreachable</span>
          </span>
        );
      case 'REJECTED':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-orange-950/80 border border-orange-700 text-orange-300">
            <ShieldOff className="w-3 h-3 text-orange-400" />
            <span>Rejected</span>
          </span>
        );
      case 'ERROR':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-rose-950/80 border border-rose-700 text-rose-300">
            <AlertTriangle className="w-3 h-3 text-rose-400" />
            <span>Error</span>
          </span>
        );
      case 'IDLE':
      default:
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-semibold bg-slate-900 border border-slate-700 text-slate-400">
            <Radio className="w-2.5 h-2.5 text-content-muted" />
            <span>Not Connected</span>
          </span>
        );
    }
  };

  const handleConnect = async (type: AppSourceType) => {
    const rawUrl = inputUrls[type]?.trim();
    if (!rawUrl) {
      setValidationErrors((prev) => ({ ...prev, [type]: 'Please provide a valid URL.' }));
      return;
    }

    // Client-side URL-syntax check: convenience only, for fast feedback —
    // the real, authoritative validation (format, scheme, and the real
    // SSRF policy) happens server-side in every case below.
    try {
      const parsed = new URL(rawUrl);
      if (!['http:', 'https:'].includes(parsed.protocol)) {
        throw new Error('URL must start with http:// or https://');
      }
      if (type === 'FIGMA' && !rawUrl.toLowerCase().includes('figma.com')) {
        setValidationErrors((prev) => ({
          ...prev,
          [type]: 'Figma link must be a valid https://www.figma.com URL.',
        }));
        return;
      }
    } catch {
      setValidationErrors((prev) => ({ ...prev, [type]: 'Invalid URL format.' }));
      onAppSourcesChange({
        ...appSources,
        [type]: {
          ...appSources[type],
          status: 'INVALID_URL',
          errorMessage: 'Invalid URL format.',
        },
      });
      return;
    }

    if (!projectId) {
      setValidationErrors((prev) => ({ ...prev, [type]: 'Select a project before verifying a connection.' }));
      return;
    }

    setValidationErrors((prev) => ({ ...prev, [type]: null }));

    onAppSourcesChange({
      ...appSources,
      [type]: {
        ...appSources[type],
        type,
        url: rawUrl,
        status: 'CHECKING',
        errorMessage: undefined,
      },
    });

    const label = SOURCES_META.find((s) => s.type === type)?.label || type;
    try {
      const result = await aiqaApi.appSource.verify(projectId, { url: rawUrl });
      onAppSourcesChange({
        ...appSources,
        [type]: {
          ...appSources[type],
          type,
          label,
          url: rawUrl,
          status: result.status,
          lastValidatedAt: new Date().toISOString(),
          details:
            result.http_status != null || result.latency_ms != null
              ? { httpStatus: result.http_status ?? undefined, latencyMs: result.latency_ms ?? undefined }
              : undefined,
          errorMessage: result.status === 'CONNECTED' ? undefined : result.message,
        },
      });
    } catch (err) {
      onAppSourcesChange({
        ...appSources,
        [type]: {
          ...appSources[type],
          type,
          label,
          url: rawUrl,
          status: 'ERROR',
          details: undefined,
          errorMessage: err instanceof ApiError ? err.message : 'Verification request failed.',
        },
      });
    }
  };

  const handleDisconnect = (type: AppSourceType) => {
    setInputUrls((prev) => ({ ...prev, [type]: '' }));
    setValidationErrors((prev) => ({ ...prev, [type]: null }));
    onAppSourcesChange({
      ...appSources,
      [type]: {
        ...appSources[type],
        url: '',
        status: 'IDLE',
        lastValidatedAt: undefined,
        details: undefined,
        errorMessage: undefined,
      },
    });
  };

  return (
    <div id="application-sources-section" className="space-y-4">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
        <div>
          <div className="flex items-center gap-2">
            <Globe className="w-4 h-4 text-violet-400" />
            <h2 className="text-sm font-bold text-slate-100 font-sans tracking-tight">
              Application Sources
            </h2>
            <span className="text-[10px] font-mono font-semibold px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-violet-300">
              4 CHANNELS
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans mt-0.5">
            Connect the application and design sources AIQA should analyze. "Connect" performs a real, bounded backend connectivity check — it does not fetch or analyze page content.
          </p>
        </div>

        {/* Environment Map Overview */}
        <div className="flex items-center gap-2 text-[10px] font-mono text-slate-400">
          <span className="text-yellow-400 font-semibold">STAGING</span>
          <span>→</span>
          <span className="text-violet-400 font-semibold">DEMO</span>
          <span>→</span>
          <span className="text-emerald-400 font-semibold">LIVE</span>
        </div>
      </div>

      {!projectId && (
        <div className="p-2 rounded bg-amber-950/40 border border-amber-800/80 text-[11px] font-mono text-amber-300 flex items-center gap-1.5">
          <AlertTriangle className="w-3.5 h-3.5 shrink-0" />
          <span>Select a project before verifying a connection — the check is a real, authenticated backend request.</span>
        </div>
      )}

      {/* 4 Cards Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        {SOURCES_META.map((meta) => {
          const currentSource = appSources[meta.type];
          const isConnected = currentSource?.status === 'CONNECTED';
          const isChecking = currentSource?.status === 'CHECKING';
          const Icon = meta.icon;
          const currentError = validationErrors[meta.type] || currentSource?.errorMessage;

          return (
            <div
              key={meta.type}
              id={`source-card-${meta.type.toLowerCase()}`}
              className={`rounded-lg p-3.5 flex flex-col justify-between transition-all duration-200 border ${
                isConnected
                  ? 'bg-[#0B1120] border-emerald-900/70 shadow-sm'
                  : 'bg-[#0B1120] border-slate-800/90 hover:border-slate-700'
              }`}
            >
              {/* Card Header */}
              <div className="space-y-1.5">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <span className="p-1.5 rounded bg-[#020617] border border-slate-800">
                      <Icon className={`w-3.5 h-3.5 ${meta.accentColor}`} />
                    </span>
                    <div>
                      <div className="flex items-center gap-1.5">
                        <span className="text-xs font-bold font-mono text-slate-200">
                          {meta.label}
                        </span>
                        <span
                          className={`text-[9px] font-mono px-1.5 py-0.2 rounded border ${meta.envBadge}`}
                        >
                          {meta.subLabel}
                        </span>
                      </div>
                    </div>
                  </div>

                  {getStatusBadge(currentSource?.status || 'IDLE')}
                </div>

                <p className="text-[11px] text-slate-400 font-sans leading-tight">
                  {meta.roleDescription}
                </p>
              </div>

              {/* URL Input Form */}
              <div className="mt-3 pt-2.5 border-t border-slate-800/80 space-y-2">
                {isConnected ? (
                  /* Connected Readout — every value below is real,
                     backend-measured/observed; nothing here is fabricated. */
                  <div className="space-y-2">
                    <div className="p-2 rounded bg-[#020617] border border-emerald-900/60 flex items-center justify-between">
                      <div className="flex items-center gap-2 truncate text-xs font-mono text-emerald-300">
                        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                        <span className="truncate">{currentSource.url}</span>
                      </div>
                      <a
                        href={currentSource.url}
                        target="_blank"
                        rel="noreferrer"
                        className="text-slate-400 hover:text-slate-200 p-1 cursor-pointer shrink-0"
                        title="Open in new tab"
                      >
                        <ExternalLink className="w-3 h-3" />
                      </a>
                    </div>

                    <div className="flex items-center justify-between text-[10px] font-mono text-slate-400">
                      <span>
                        {currentSource.details?.httpStatus != null ? `HTTP ${currentSource.details.httpStatus}` : 'Reachable'}
                        {currentSource.details?.latencyMs != null ? ` • Response time: ${currentSource.details.latencyMs}ms` : ''}
                      </span>
                      <button
                        type="button"
                        onClick={() => handleDisconnect(meta.type)}
                        disabled={disabled}
                        className="text-rose-400 hover:text-rose-300 font-medium cursor-pointer transition-colors flex items-center gap-1 disabled:opacity-50"
                      >
                        <Unlink className="w-3 h-3" />
                        <span>Disconnect</span>
                      </button>
                    </div>
                  </div>
                ) : (
                  /* Disconnected Input */
                  <div className="space-y-1.5">
                    <div className="flex items-center gap-1.5">
                      <input
                        type="url"
                        placeholder={meta.placeholder}
                        value={inputUrls[meta.type]}
                        onChange={(e) => {
                          setInputUrls((prev) => ({ ...prev, [meta.type]: e.target.value }));
                          setValidationErrors((prev) => ({ ...prev, [meta.type]: null }));
                        }}
                        disabled={disabled || isChecking}
                        className="flex-1 px-2.5 py-1.5 rounded bg-[#020617] border border-slate-700 hover:border-slate-600 focus:border-violet-500 text-xs font-mono text-slate-100 placeholder-slate-500 focus:outline-none"
                      />
                      <button
                        type="button"
                        onClick={() => handleConnect(meta.type)}
                        disabled={disabled || isChecking || !projectId || !inputUrls[meta.type]?.trim()}
                        className="px-3 py-1.5 rounded bg-violet-600 hover:bg-violet-500 disabled:bg-slate-800 text-white disabled:text-content-muted text-xs font-mono font-medium cursor-pointer transition-colors flex items-center gap-1 shrink-0"
                      >
                        {isChecking ? (
                          <>
                            <RefreshCw className="w-3 h-3 animate-spin" />
                            <span>Checking…</span>
                          </>
                        ) : (
                          <>
                            <Link2 className="w-3 h-3" />
                            <span>Connect</span>
                          </>
                        )}
                      </button>
                    </div>

                    {meta.type === 'FIGMA' && !isConnected && (
                      <div className="text-[10px] font-mono text-content-muted">
                        No Figma source connected
                      </div>
                    )}
                  </div>
                )}

                {/* Validation / Real Verification Error Banner */}
                {currentError && (
                  <div className="p-1.5 rounded bg-rose-950/40 border border-rose-800/80 text-[10px] font-mono text-rose-300 flex items-center gap-1.5">
                    <AlertTriangle className="w-3 h-3 text-rose-400 shrink-0" />
                    <span>{currentError}</span>
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

/**
 * Mobile Testing — the real device fleet on this host.
 *
 * History: this page shipped a fabricated device fleet (hardcoded
 * `Samsung Galaxy S24 Ultra`, `DEV-AND-01`, a "Lock & Reserve Device"
 * button that reserved nothing). FE-11 replaced that with an honest
 * "deferred" state, correctly — there was no device-inventory entity.
 *
 * There still is no device *inventory table*, and this page still does not
 * invent one. What it does instead is ask the host what is genuinely there,
 * every time, via `GET /api/v1/mobile/fleet` (`mobile.rs`):
 *
 *   - `adb devices -l` — the real devices/emulators the Android Debug
 *     Bridge can see right now, with their real connection state. An
 *     `unauthorized` handset is shown as unauthorized, not as usable,
 *     because it genuinely cannot run a test until the USB-debugging prompt
 *     is accepted on the device itself.
 *   - `emulator -list-avds` — the AVDs that really exist on this machine.
 *     An AVD is a *definition*, so it is listed separately from live
 *     devices; showing them together would present a fleet that is not
 *     actually running. Booting one is a real `emulator -avd <name>` spawn.
 *   - `xcrun simctl list devices available --json` — real iOS Simulators.
 *     macOS only, and that limit is stated as a platform fact rather than a
 *     missing install, because Apple ships no Simulator runtime elsewhere.
 *   - `idevice_id -l` (libimobiledevice) — real *physical* iPhones/iPads,
 *     which unlike Simulators genuinely can attach to a Windows host.
 *   - `maestro --version` — whether the real mobile.dev Maestro CLI is
 *     actually installed, using the same identity check the executor
 *     applies before it will run anything. If Maestro is missing, this page
 *     says dispatch is unavailable rather than offering a button that would
 *     fail at execution time.
 *
 * Zero devices is a real, expected answer and is rendered as exactly that.
 * Nothing on this page is seeded, cached, or placeheld.
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  AlertTriangle,
  CheckCircle2,
  Info,
  Loader2,
  PlayCircle,
  RefreshCw,
  Smartphone,
  TabletSmartphone,
  TerminalSquare,
  Usb,
} from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { OrchestratorLogsPanel } from '../dashboard/OrchestratorLogsPanel';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import { useAuth } from '../../context/AuthContext';
import { useProject } from '../../context/ProjectContext';
import { renderAuthState, renderResourceErrorState } from '../state/StateViews';
import type { MobileFleet, MobileToolStatus } from '../../types/contract';

/** Real adb states, mapped to how each should read. Anything adb reports
 *  that is not in this list is still shown verbatim — never coerced into a
 *  friendlier-looking state it is not in. */
const stateStyles: Record<string, string> = {
  device: 'text-emerald-300 bg-emerald-950/70 border-emerald-800',
  offline: 'text-slate-400 bg-slate-800/70 border-slate-700',
  unauthorized: 'text-amber-300 bg-amber-950/70 border-amber-800',
};

const Pill: React.FC<{ label: string; className?: string }> = ({ label, className }) => (
  <span
    className={`inline-flex items-center px-1.5 py-0.5 rounded border text-[9px] font-mono font-bold uppercase tracking-wide shrink-0 ${
      className ?? 'text-slate-400 bg-slate-800/60 border-slate-700'
    }`}
  >
    {label}
  </span>
);

const ToolRow: React.FC<{ name: string; tool: MobileToolStatus }> = ({ name, tool }) => (
  <div
    id={`mobile-tool-${name.toLowerCase()}`}
    className="flex items-start justify-between gap-2 px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800"
  >
    <div className="min-w-0">
      <div className="text-[11px] font-mono text-slate-200">{name}</div>
      {/* The backend's real detail: a version string, or the real reason
          the tool cannot be used. */}
      <div className="text-[10px] font-mono text-content-muted break-all">{tool.detail}</div>
      {tool.path && <div className="text-[9px] font-mono text-content-muted break-all">{tool.path}</div>}
    </div>
    <Pill
      label={tool.available ? 'available' : 'missing'}
      className={
        tool.available
          ? 'text-emerald-300 bg-emerald-950/70 border-emerald-800'
          : 'text-rose-300 bg-rose-950/70 border-rose-800'
      }
    />
  </div>
);

export const MobileTestingPage: React.FC = () => {
  const navigate = useNavigate();
  const auth = useAuth();
  const { selectedProjectId } = useProject();

  const [fleet, setFleet] = useState<MobileFleet | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [loading, setLoading] = useState(false);
  const [bootingAvd, setBootingAvd] = useState<string | null>(null);
  const [bootingUdid, setBootingUdid] = useState<string | null>(null);
  const [bootMessage, setBootMessage] = useState<string | null>(null);
  const [bootError, setBootError] = useState<string | null>(null);

  const isUnmounted = useRef(false);

  const load = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    try {
      const f = await aiqaApi.mobile.fleet(signal);
      if (signal?.aborted || isUnmounted.current) return;
      // Structural guard — an unexpected payload must not crash the page.
      if (!f || !Array.isArray(f.devices) || !Array.isArray(f.avds)) {
        setFleet(null);
        setError(null);
        return;
      }
      setFleet(f);
      setError(null);
    } catch (err) {
      if (signal?.aborted || isUnmounted.current) return;
      setFleet(null);
      if (err instanceof ApiError) setError(err);
    } finally {
      if (!signal?.aborted && !isUnmounted.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    isUnmounted.current = false;
    if (auth.status !== 'authenticated') return;
    const controller = new AbortController();
    void load(controller.signal);
    // A device being plugged in or an emulator finishing boot is a real
    // host-level event with no backend push channel, so this lane is
    // genuinely polled — and is labelled as polled in the UI.
    const timer = setInterval(() => void load(controller.signal), 6000);
    return () => {
      isUnmounted.current = true;
      controller.abort();
      clearInterval(timer);
    };
  }, [auth.status, load]);

  const bootEmulator = useCallback(
    async (avd: string) => {
      setBootingAvd(avd);
      setBootError(null);
      setBootMessage(null);
      try {
        const res = await aiqaApi.mobile.bootEmulator(avd);
        if (isUnmounted.current) return;
        // Deliberately not "booted": only adb can prove readiness, and it
        // takes tens of seconds. The list below is what actually confirms it.
        setBootMessage(res.message);
      } catch (err) {
        if (isUnmounted.current) return;
        setBootError(err instanceof Error ? err.message : 'Failed to start the emulator.');
      } finally {
        if (!isUnmounted.current) setBootingAvd(null);
      }
    },
    [],
  );

  const bootSimulator = useCallback(async (udid: string) => {
    setBootingUdid(udid);
    setBootError(null);
    setBootMessage(null);
    try {
      const res = await aiqaApi.mobile.bootSimulator(udid);
      if (isUnmounted.current) return;
      setBootMessage(res.message);
    } catch (err) {
      if (isUnmounted.current) return;
      setBootError(err instanceof Error ? err.message : 'Failed to boot the simulator.');
    } finally {
      if (!isUnmounted.current) setBootingUdid(null);
    }
  }, []);

  if (auth.status !== 'authenticated') {
    return (
      <div id="mobile-testing-page" className="space-y-4 animate-in fade-in duration-150">
        <SectionHeader
          title="Mobile Testing"
          subtitle="Real devices and emulators visible to this host"
          badge="DEVICES"
          icon={Smartphone}
        />
        {renderAuthState(auth)}
      </div>
    );
  }

  // Readiness counts both platforms: Maestro drives Android and iOS alike,
  // so a booted Simulator is just as dispatchable as a ready handset.
  const readyAndroid = fleet?.devices.filter((d) => d.ready).length ?? 0;
  const readyIos = fleet?.ios_devices.filter((d) => d.ready).length ?? 0;
  const readyCount = readyAndroid + readyIos;
  const canDispatch = (fleet?.maestro.available ?? false) && readyCount > 0;

  return (
    <div id="mobile-testing-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Mobile Testing"
        subtitle="Real devices and emulators visible to this host"
        badge="DEVICES"
        icon={Smartphone}
        actions={
          <div className="flex items-center gap-2">
            <Pill label="polled 6s" />
            <button
              type="button"
              id="btn-mobile-refresh"
              onClick={() => void load()}
              disabled={loading}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] border border-slate-700 hover:border-violet-600 text-slate-300 hover:text-violet-300 font-mono text-xs font-semibold cursor-pointer transition-colors disabled:opacity-50"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
              Refresh
            </button>
          </div>
        }
      />

      <div className="grid grid-cols-1 xl:grid-cols-[minmax(0,1fr)_400px] gap-4 items-start">
        <div className="space-y-4 min-w-0">
          {error ? (
            renderResourceErrorState(error, () => void load(), {
              forbiddenTitle: 'Not permitted to read the device fleet',
              forbiddenDescription: 'The orchestrator refused this request.',
              offlineDescriptionSuffix: 'to read the real device fleet.',
              genericTitle: "Couldn't read the device fleet",
            })
          ) : !fleet ? (
            <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-6 flex items-center justify-center gap-2 text-slate-400 text-xs font-mono">
              <Loader2 className="w-4 h-4 animate-spin text-violet-400" />
              Asking the host for real devices…
            </div>
          ) : (
            <>
              {/* ------------------------------------ connected devices */}
              <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-3">
                <div className="flex items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
                  <div className="flex items-center gap-2 min-w-0">
                    <Usb className="w-4 h-4 text-emerald-400 shrink-0" />
                    <div className="min-w-0">
                      <h2 className="text-sm font-bold text-slate-100 tracking-tight">Connected Devices</h2>
                      <p className="text-[11px] text-content-muted">Live from `adb devices -l` on this host</p>
                    </div>
                  </div>
                  <Pill label={`${readyCount} ready`} />
                </div>

                {fleet.devices.length === 0 ? (
                  <div className="rounded-md bg-[#020617] border border-slate-800 p-3 flex items-start gap-2">
                    <Info className="w-3.5 h-3.5 text-content-muted shrink-0 mt-0.5" />
                    <p className="text-[11px] text-slate-400">
                      No device or emulator is currently connected. Plug in a handset with USB debugging enabled,
                      or boot one of the real AVDs below — this list updates as soon as adb sees it. Nothing is
                      shown here that is not genuinely attached.
                    </p>
                  </div>
                ) : (
                  <ul className="space-y-1">
                    {fleet.devices.map((d) => (
                      <li
                        key={d.serial}
                        id={`mobile-device-${d.serial}`}
                        className="flex items-center justify-between gap-2 px-2.5 py-2 rounded-md bg-[#020617] border border-slate-800"
                      >
                        <div className="min-w-0">
                          <div className="flex items-center gap-2 flex-wrap">
                            <span className="text-xs font-mono font-bold text-slate-100 truncate">{d.serial}</span>
                            <Pill label={d.emulator ? 'emulator' : 'physical'} />
                          </div>
                          <div className="text-[10px] font-mono text-content-muted">
                            {d.model ?? 'model not reported by adb'}
                          </div>
                          {d.state === 'unauthorized' && (
                            <div className="text-[10px] font-mono text-amber-400/90 mt-0.5">
                              Accept the USB debugging prompt on the device — it cannot run a test until then.
                            </div>
                          )}
                        </div>
                        {/* adb's own state string, verbatim. */}
                        <Pill label={d.state} className={stateStyles[d.state]} />
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              {/* ------------------------------------------ real AVDs */}
              <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-3">
                <div className="flex items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
                  <div className="flex items-center gap-2 min-w-0">
                    <Smartphone className="w-4 h-4 text-violet-400 shrink-0" />
                    <div className="min-w-0">
                      <h2 className="text-sm font-bold text-slate-100 tracking-tight">Available Emulators</h2>
                      <p className="text-[11px] text-content-muted">Real AVDs on this machine — booting one is real</p>
                    </div>
                  </div>
                  <Pill label={`${fleet.avds.length} avd${fleet.avds.length === 1 ? '' : 's'}`} />
                </div>

                {fleet.avds.length === 0 ? (
                  <p className="text-[11px] text-slate-400">
                    No AVDs are defined on this host. Create one in Android Studio and it will appear here.
                  </p>
                ) : (
                  <ul className="space-y-1">
                    {fleet.avds.map((a) => (
                      <li
                        key={a.name}
                        id={`mobile-avd-${a.name}`}
                        className="flex items-center justify-between gap-2 px-2.5 py-2 rounded-md bg-[#020617] border border-slate-800"
                      >
                        <div className="min-w-0 flex items-center gap-2">
                          <span className="text-xs font-mono text-slate-200 truncate">{a.name}</span>
                          {a.running && (
                            <Pill label="running" className="text-emerald-300 bg-emerald-950/70 border-emerald-800" />
                          )}
                        </div>
                        {!a.running && (
                          <button
                            type="button"
                            id={`btn-boot-avd-${a.name}`}
                            onClick={() => void bootEmulator(a.name)}
                            disabled={bootingAvd !== null || !fleet.emulator_tool.available}
                            className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-violet-600 hover:bg-violet-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-mono text-[11px] font-semibold cursor-pointer"
                          >
                            {bootingAvd === a.name ? (
                              <Loader2 className="w-3 h-3 animate-spin" />
                            ) : (
                              <PlayCircle className="w-3 h-3" />
                            )}
                            Boot
                          </button>
                        )}
                      </li>
                    ))}
                  </ul>
                )}

                {bootMessage && (
                  <p className="text-[10px] font-mono text-yellow-300 flex items-start gap-1.5">
                    <Info className="w-3 h-3 shrink-0 mt-0.5" />
                    <span>
                      {bootMessage}. Boot takes a while — it will appear above as a real device once adb sees it.
                    </span>
                  </p>
                )}
                {bootError && (
                  <p className="text-[10px] font-mono text-rose-300 flex items-start gap-1.5">
                    <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
                    <span>{bootError}</span>
                  </p>
                )}
              </div>

              {/* ------------------------------------------ iOS devices */}
              <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-3">
                <div className="flex items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
                  <div className="flex items-center gap-2 min-w-0">
                    <TabletSmartphone className="w-4 h-4 text-yellow-400 shrink-0" />
                    <div className="min-w-0">
                      <h2 className="text-sm font-bold text-slate-100 tracking-tight">iOS Devices</h2>
                      <p className="text-[11px] text-content-muted">
                        Real Simulators (`xcrun simctl`) and attached hardware (libimobiledevice)
                      </p>
                    </div>
                  </div>
                  <Pill label={`${readyIos} ready`} />
                </div>

                {fleet.ios_devices.length === 0 ? (
                  <div
                    id="ios-unavailable-notice"
                    className="rounded-md bg-[#020617] border border-slate-800 p-3 space-y-1.5"
                  >
                    {/* The real reason, from the backend — a platform limit is
                        stated as one, never as an empty fleet that implies a
                        simulator could simply be created. */}
                    {!fleet.simctl.available && (
                      <p className="text-[11px] text-slate-400 flex items-start gap-2">
                        <Info className="w-3.5 h-3.5 text-content-muted shrink-0 mt-0.5" />
                        <span>{fleet.simctl.detail}</span>
                      </p>
                    )}
                    {!fleet.libimobiledevice.available && (
                      <p className="text-[11px] text-slate-400 flex items-start gap-2">
                        <Info className="w-3.5 h-3.5 text-content-muted shrink-0 mt-0.5" />
                        <span>
                          {fleet.libimobiledevice.detail} A physical iPhone/iPad can be used from this{' '}
                          <span className="font-mono text-slate-300">{fleet.host_os}</span> host once it is
                          installed.
                        </span>
                      </p>
                    )}
                    {fleet.simctl.available && fleet.libimobiledevice.available && (
                      <p className="text-[11px] text-slate-400">
                        No iOS Simulator or attached device is currently available. Nothing is listed that is not
                        genuinely there.
                      </p>
                    )}
                  </div>
                ) : (
                  <ul className="space-y-1">
                    {fleet.ios_devices.map((d) => (
                      <li
                        key={d.udid}
                        id={`ios-device-${d.udid}`}
                        className="flex items-center justify-between gap-2 px-2.5 py-2 rounded-md bg-[#020617] border border-slate-800"
                      >
                        <div className="min-w-0">
                          <div className="flex items-center gap-2 flex-wrap">
                            <span className="text-xs font-mono font-bold text-slate-100 truncate">{d.name}</span>
                            <Pill label={d.simulator ? 'simulator' : 'physical'} />
                          </div>
                          <div className="text-[10px] font-mono text-content-muted truncate">
                            {d.runtime ?? 'runtime not reported'} · {d.udid}
                          </div>
                        </div>
                        <div className="flex items-center gap-1.5 shrink-0">
                          <Pill
                            label={d.state}
                            className={
                              d.ready
                                ? 'text-emerald-300 bg-emerald-950/70 border-emerald-800'
                                : 'text-slate-400 bg-slate-800/70 border-slate-700'
                            }
                          />
                          {d.simulator && !d.ready && (
                            <button
                              type="button"
                              id={`btn-boot-sim-${d.udid}`}
                              onClick={() => void bootSimulator(d.udid)}
                              disabled={bootingUdid !== null}
                              className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-violet-600 hover:bg-violet-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-mono text-[11px] font-semibold cursor-pointer"
                            >
                              {bootingUdid === d.udid ? (
                                <Loader2 className="w-3 h-3 animate-spin" />
                              ) : (
                                <PlayCircle className="w-3 h-3" />
                              )}
                              Boot
                            </button>
                          )}
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              {/* -------------------------------------------- toolchain */}
              <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-3">
                <div className="flex items-center gap-2 pb-2 border-b border-slate-800/80">
                  <TerminalSquare className="w-4 h-4 text-yellow-400 shrink-0" />
                  <div className="min-w-0">
                    <h2 className="text-sm font-bold text-slate-100 tracking-tight">Host Toolchain</h2>
                    <p className="text-[11px] text-content-muted">Real binaries this host resolved, with real versions</p>
                  </div>
                </div>
                <div className="space-y-1">
                  <ToolRow name="adb" tool={fleet.adb} />
                  <ToolRow name="emulator" tool={fleet.emulator_tool} />
                  <ToolRow name="maestro" tool={fleet.maestro} />
                  <ToolRow name="simctl" tool={fleet.simctl} />
                  <ToolRow name="libimobiledevice" tool={fleet.libimobiledevice} />
                </div>
              </div>

              {/* -------------------------------------------- dispatch */}
              <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-2">
                <div className="flex items-center gap-2">
                  {canDispatch ? (
                    <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                  ) : (
                    <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0" />
                  )}
                  <h2 className="text-sm font-bold text-slate-100 tracking-tight">Run a mobile test</h2>
                </div>

                {canDispatch ? (
                  <p className="text-[11px] text-slate-400">
                    Maestro is installed and {readyCount} device{readyCount === 1 ? ' is' : 's are'} ready. Create
                    an execution with the <span className="font-mono text-slate-300">maestro</span> executor in the
                    Execution Hub — that is the same real dispatch path every other executor uses. Mobile runs are
                    policy-gated: the run will wait in <span className="font-mono text-slate-300">PENDING_APPROVAL</span>{' '}
                    until approved, because this adapter can touch a real device.
                  </p>
                ) : (
                  <p className="text-[11px] text-slate-400">
                    {!fleet.maestro.available
                      ? `Dispatch is unavailable: ${fleet.maestro.detail}. No button is offered for a run that would fail at execution time.`
                      : 'Dispatch is unavailable: no Android device is in the adb `device` state and no iOS device is ready, so a Maestro flow has nothing to run against.'}
                  </p>
                )}

                <button
                  type="button"
                  id="btn-mobile-open-execution-hub"
                  onClick={() => navigate('/execution')}
                  disabled={!canDispatch}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-semibold text-xs cursor-pointer"
                >
                  <PlayCircle className="w-3.5 h-3.5" />
                  Open Execution Hub
                </button>
              </div>
            </>
          )}
        </div>

        <div className="min-w-0 xl:sticky xl:top-4">
          <OrchestratorLogsPanel projectId={selectedProjectId} />
        </div>
      </div>
    </div>
  );
};

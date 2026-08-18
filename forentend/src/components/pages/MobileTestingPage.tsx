/**
 * AIQA Mobile Testing Page
 * Select compatible Android/Samsung or iOS/Apple devices, lock resources,
 * and execute eligible non-UI/UX mobile coverage.
 */

import React, { useState } from 'react';
import {
  Smartphone,
  Tablet,
  Lock,
  Unlock,
  Play,
  Activity,
  Cpu,
  Battery,
  Flame,
  CheckCircle2,
  RefreshCw,
  Sliders,
  Terminal,
  Shield,
  Layers,
  Sparkles,
  Wifi,
  Database,
  Radio,
  Key,
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { SectionHeader } from '../common/SectionHeader';
import { StatusBadge } from '../common/StatusBadge';
import { MobileDeviceTarget, MobileTestExecution } from '../../types';

export const MobileTestingPage: React.FC = () => {
  const { addActivity } = useAIQA();

  const [devices, setDevices] = useState<MobileDeviceTarget[]>([
    {
      id: 'DEV-AND-01',
      name: 'Samsung Galaxy S24 Ultra',
      platform: 'ANDROID',
      osVersion: 'Android 14 (OneUI 6.1)',
      manufacturer: 'Samsung',
      screenDensity: 'xxxhdpi (505ppi)',
      status: 'AVAILABLE',
      batteryLevel: 94,
      thermalStatus: 'NOMINAL',
    },
    {
      id: 'DEV-AND-02',
      name: 'Samsung Galaxy A54 5G',
      platform: 'ANDROID',
      osVersion: 'Android 14',
      manufacturer: 'Samsung',
      screenDensity: 'xxhdpi (403ppi)',
      status: 'LOCKED_RESERVED',
      batteryLevel: 82,
      thermalStatus: 'NOMINAL',
      lockedBy: 'CI-Runner-04',
    },
    {
      id: 'DEV-AND-03',
      name: 'Google Pixel 9 Pro',
      platform: 'ANDROID',
      osVersion: 'Android 15 Developer Preview',
      manufacturer: 'Google',
      screenDensity: 'xxxhdpi (495ppi)',
      status: 'EXECUTING',
      batteryLevel: 78,
      thermalStatus: 'WARM',
      lockedBy: 'AIQA-Executor-Pool',
    },
    {
      id: 'DEV-IOS-01',
      name: 'Apple iPhone 16 Pro',
      platform: 'IOS',
      osVersion: 'iOS 18.2',
      manufacturer: 'Apple',
      screenDensity: '@3x (460ppi)',
      status: 'AVAILABLE',
      batteryLevel: 98,
      thermalStatus: 'NOMINAL',
    },
    {
      id: 'DEV-IOS-02',
      name: 'Apple iPhone 15',
      platform: 'IOS',
      osVersion: 'iOS 17.5',
      manufacturer: 'Apple',
      screenDensity: '@3x (460ppi)',
      status: 'AVAILABLE',
      batteryLevel: 89,
      thermalStatus: 'NOMINAL',
    },
    {
      id: 'DEV-IOS-03',
      name: 'Apple iPad Pro 12.9" M2',
      platform: 'IOS',
      osVersion: 'iPadOS 17.6',
      manufacturer: 'Apple',
      screenDensity: '@2x (264ppi)',
      status: 'AVAILABLE',
      batteryLevel: 91,
      thermalStatus: 'NOMINAL',
    },
  ]);

  const [selectedDeviceId, setSelectedDeviceId] = useState<string>('DEV-AND-01');
  const [platformFilter, setPlatformFilter] = useState<'ALL' | 'ANDROID' | 'IOS'>('ALL');

  const [testExecutions, setTestExecutions] = useState<MobileTestExecution[]>([
    {
      id: 'MOB-EXEC-401',
      title: 'Deep Link Routing & JWT Refresh on Universal Link Ingestion',
      deviceId: 'DEV-AND-01',
      coverageType: 'DEEP_LINKING',
      status: 'PASSED',
      durationMs: 4200,
      deviceMetrics: { cpuPeakPct: 18, ramUsedMb: 142, networkPackets: 1280, batteryDrainPct: 0.1 },
      logSummary: 'Universal link dispatched via intent filter. Token exchange verified against auth server.',
    },
    {
      id: 'MOB-EXEC-402',
      title: 'Offline SQLite Sync & Conflict Resolution during Airplane Mode',
      deviceId: 'DEV-AND-01',
      coverageType: 'OFFLINE_CACHE_SYNC',
      status: 'PASSED',
      durationMs: 7800,
      deviceMetrics: { cpuPeakPct: 34, ramUsedMb: 210, networkPackets: 450, batteryDrainPct: 0.3 },
      logSummary: '35 mutations queued offline. Re-connection trigger successfully flushed all items with timestamp integrity.',
    },
    {
      id: 'MOB-EXEC-403',
      title: 'Biometric Keystore Key Attestation & Secure Enclave Payload',
      deviceId: 'DEV-IOS-01',
      coverageType: 'BIOMETRIC_TOKEN_AUTH',
      status: 'PASSED',
      durationMs: 2900,
      deviceMetrics: { cpuPeakPct: 12, ramUsedMb: 98, networkPackets: 320, batteryDrainPct: 0.05 },
      logSummary: 'Secure enclave crypto challenge signed successfully. Hardware-backed key validated.',
    },
    {
      id: 'MOB-EXEC-404',
      title: 'Silent Push Notification Wakeup & Background DB Sync',
      deviceId: 'DEV-AND-03',
      coverageType: 'PUSH_NOTIFICATION_PIPELINE',
      status: 'RUNNING',
      durationMs: 3100,
      deviceMetrics: { cpuPeakPct: 28, ramUsedMb: 180, networkPackets: 890, batteryDrainPct: 0.2 },
      logSummary: 'FCM High-Priority payload received. Background headless task executing data delta fetch.',
    },
  ]);

  const [isRunningCoverage, setIsRunningCoverage] = useState(false);
  const [activeCoverageType, setActiveCoverageType] = useState<MobileTestExecution['coverageType']>('DEEP_LINKING');

  const selectedDevice = devices.find((d) => d.id === selectedDeviceId) || devices[0];

  const handleToggleLock = (deviceId: string) => {
    setDevices(
      devices.map((d) => {
        if (d.id === deviceId) {
          const isLocked = d.status === 'LOCKED_RESERVED';
          return {
            ...d,
            status: isLocked ? 'AVAILABLE' : 'LOCKED_RESERVED',
            lockedBy: isLocked ? undefined : 'Current User (QA Lead)',
          };
        }
        return d;
      })
    );
  };

  const handleRunMobileTest = () => {
    setIsRunningCoverage(true);

    const newExec: MobileTestExecution = {
      id: `MOB-EXEC-${Math.floor(500 + Math.random() * 400)}`,
      title: `Automated Coverage: ${activeCoverageType.replace(/_/g, ' ')}`,
      deviceId: selectedDevice.id,
      coverageType: activeCoverageType,
      status: 'RUNNING',
      durationMs: 0,
      deviceMetrics: {
        cpuPeakPct: Math.floor(15 + Math.random() * 30),
        ramUsedMb: Math.floor(120 + Math.random() * 150),
        networkPackets: Math.floor(500 + Math.random() * 1000),
        batteryDrainPct: 0.2,
      },
      logSummary: `Executing non-UI headless test suite on ${selectedDevice.name} (${selectedDevice.osVersion})...`,
    };

    setTestExecutions([newExec, ...testExecutions]);

    setTimeout(() => {
      setTestExecutions((prev) =>
        prev.map((e) =>
          e.id === newExec.id
            ? {
                ...e,
                status: 'PASSED',
                durationMs: 4500,
                logSummary: `Execution complete on ${selectedDevice.name}. Zero UI frame overhead. Assertions passed: 18/18.`,
              }
            : e
        )
      );
      setIsRunningCoverage(false);
    }, 2000);

    addActivity({
      type: 'RUN_STARTED',
      title: `Mobile Headless Coverage Run: ${newExec.id}`,
      description: `Dispatched ${activeCoverageType} to ${selectedDevice.name}.`,
      actor: 'Mobile Test Coordinator',
    });
  };

  const filteredDevices = devices.filter((d) =>
    platformFilter === 'ALL' ? true : d.platform === platformFilter
  );

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Mobile Testing Hub"
        subtitle="Select compatible Android (Samsung/Pixel) or iOS (Apple) devices, lock resource pools, and execute eligible non-UI/UX mobile test suites"
        actions={
          <div className="flex items-center gap-2">
            <button
              onClick={() => handleToggleLock(selectedDevice.id)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 text-xs font-semibold border border-slate-800 cursor-pointer"
            >
              {selectedDevice.status === 'LOCKED_RESERVED' ? (
                <>
                  <Unlock className="w-3.5 h-3.5 text-yellow-400" />
                  <span>Release Device Lock</span>
                </>
              ) : (
                <>
                  <Lock className="w-3.5 h-3.5 text-indigo-400" />
                  <span>Lock & Reserve Device</span>
                </>
              )}
            </button>
            <button
              onClick={handleRunMobileTest}
              disabled={isRunningCoverage || selectedDevice.status === 'OFFLINE'}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer disabled:opacity-50"
            >
              <Play className="w-3.5 h-3.5" />
              <span>{isRunningCoverage ? 'Executing Suite...' : 'Execute Non-UI Suite'}</span>
            </button>
          </div>
        }
      />

      {/* Mobile Top Fleet Metrics */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2.5">
        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Device Pool Fleet</span>
            <Smartphone className="w-3.5 h-3.5 text-indigo-400" />
          </div>
          <div className="text-lg font-bold font-mono text-slate-100">
            {devices.filter((d) => d.status === 'AVAILABLE').length} Available / {devices.length} Total
          </div>
          <div className="text-[10px] text-slate-500">Samsung, Pixel, iPhone, iPad</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Locked / Dedicated</span>
            <Lock className="w-3.5 h-3.5 text-yellow-400" />
          </div>
          <div className="text-lg font-bold font-mono text-yellow-300">
            {devices.filter((d) => d.status === 'LOCKED_RESERVED').length} Device Pools
          </div>
          <div className="text-[10px] text-slate-500">Guaranteed non-collision</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Headless Test Pass Rate</span>
            <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
          </div>
          <div className="text-lg font-bold font-mono text-emerald-400">98.4% Passed</div>
          <div className="text-[10px] text-slate-500">Non-UI/UX background logic</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Thermal & Battery Health</span>
            <Battery className="w-3.5 h-3.5 text-emerald-400" />
          </div>
          <div className="text-lg font-bold font-mono text-emerald-400">Nominal Fleet</div>
          <div className="text-[10px] text-slate-500">Avg battery 88% • 0 throttled</div>
        </div>
      </div>

      {/* Main Grid: Device Fleet & Non-UI Coverage Launcher */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
        {/* Left Column: Device Fleet Selection (5 cols) */}
        <div className="lg:col-span-5 rounded-lg border border-slate-800 bg-[#0B1120] p-3 space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-slate-800">
            <span className="text-xs font-bold text-slate-200 uppercase font-mono">
              Device Cloud Target Pool ({filteredDevices.length})
            </span>
            <div className="flex items-center gap-1 bg-[#020617] p-0.5 rounded border border-slate-800 text-[10px] font-mono">
              {(['ALL', 'ANDROID', 'IOS'] as const).map((pf) => (
                <button
                  key={pf}
                  onClick={() => setPlatformFilter(pf)}
                  className={`px-2 py-0.5 rounded cursor-pointer ${
                    platformFilter === pf
                      ? 'bg-indigo-600 text-white font-bold'
                      : 'text-slate-400 hover:text-slate-200'
                  }`}
                >
                  {pf}
                </button>
              ))}
            </div>
          </div>

          <div className="space-y-2">
            {filteredDevices.map((device) => {
              const isSelected = device.id === selectedDeviceId;
              return (
                <div
                  key={device.id}
                  onClick={() => setSelectedDeviceId(device.id)}
                  className={`p-2.5 rounded-lg border text-xs cursor-pointer transition-all ${
                    isSelected
                      ? 'border-indigo-500 bg-indigo-950/40'
                      : 'border-slate-800 bg-[#020617] hover:border-slate-700'
                  }`}
                >
                  <div className="flex items-start justify-between gap-2 mb-1">
                    <div className="space-y-0.5">
                      <div className="flex items-center gap-1.5">
                        <span className="font-mono font-bold text-indigo-400 text-xs">{device.id}</span>
                        <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-slate-800 text-slate-300 border border-slate-700">
                          {device.manufacturer}
                        </span>
                        <span
                          className={`text-[9px] font-mono px-1.5 py-0.2 rounded font-bold ${
                            device.platform === 'ANDROID' ? 'text-emerald-300' : 'text-cyan-300'
                          }`}
                        >
                          {device.platform}
                        </span>
                      </div>
                      <h4 className="font-semibold text-slate-100 text-xs">{device.name}</h4>
                    </div>

                    <span
                      className={`text-[9px] font-mono px-2 py-0.5 rounded font-bold shrink-0 ${
                        device.status === 'AVAILABLE'
                          ? 'bg-emerald-950 text-emerald-300 border border-emerald-800'
                          : device.status === 'LOCKED_RESERVED'
                          ? 'bg-yellow-950 text-yellow-300 border border-yellow-800'
                          : 'bg-indigo-950 text-indigo-300 border border-indigo-800 animate-pulse'
                      }`}
                    >
                      {device.status.replace('_', ' ')}
                    </span>
                  </div>

                  <div className="flex items-center justify-between text-[10px] font-mono text-slate-400 pt-1 border-t border-slate-800/80">
                    <span>OS: {device.osVersion}</span>
                    <div className="flex items-center gap-2 text-slate-300">
                      <span className="flex items-center gap-0.5">
                        <Battery className="w-3 h-3 text-emerald-400" />
                        {device.batteryLevel}%
                      </span>
                      <span className="text-slate-500">|</span>
                      <span>{device.thermalStatus}</span>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Right Column: Non-UI Mobile Coverage Suites & Telemetry (7 cols) */}
        <div className="lg:col-span-7 rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-4 text-xs">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-slate-800">
            <div>
              <div className="flex items-center gap-2">
                <span className="font-mono font-bold text-indigo-400">{selectedDevice.id}</span>
                <h3 className="text-sm font-bold text-slate-100">{selectedDevice.name}</h3>
              </div>
              <p className="text-xs text-slate-400 font-mono mt-0.5">
                {selectedDevice.osVersion} • Density: {selectedDevice.screenDensity}
              </p>
            </div>
            {selectedDevice.lockedBy && (
              <span className="text-[10px] font-mono text-yellow-400 bg-yellow-950/60 px-2 py-0.5 rounded border border-yellow-800">
                LOCKED: {selectedDevice.lockedBy}
              </span>
            )}
          </div>

          {/* Non-UI Coverage Selector */}
          <div className="space-y-2">
            <label className="block text-slate-300 font-medium font-mono text-[11px] uppercase">
              Select Eligible Non-UI/UX Mobile Coverage:
            </label>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
              {[
                {
                  id: 'DEEP_LINKING',
                  title: 'Deep Link & Universal Routing',
                  desc: 'Validates URI intents & JWT token session exchange without rendering DOM',
                  icon: Radio,
                },
                {
                  id: 'OFFLINE_CACHE_SYNC',
                  title: 'Offline SQLite & Data Sync',
                  desc: 'Queues mutations offline; tests eventual consistency and conflict logic',
                  icon: Database,
                },
                {
                  id: 'BIOMETRIC_TOKEN_AUTH',
                  title: 'Biometric Keystore Auth',
                  desc: 'Simulates FaceID/Keystore hardware enclave crypto challenge signing',
                  icon: Key,
                },
                {
                  id: 'PUSH_NOTIFICATION_PIPELINE',
                  title: 'Push Notification Payload',
                  desc: 'APNs & FCM silent payload ingestion and background state trigger',
                  icon: Wifi,
                },
              ].map((cov) => {
                const isSelected = activeCoverageType === cov.id;
                const Icon = cov.icon;
                return (
                  <div
                    key={cov.id}
                    onClick={() => setActiveCoverageType(cov.id as any)}
                    className={`p-2.5 rounded-lg border cursor-pointer transition-all ${
                      isSelected
                        ? 'border-indigo-500 bg-indigo-950/50 text-slate-100 shadow-sm'
                        : 'border-slate-800 bg-[#020617] hover:border-slate-700 text-slate-400'
                    }`}
                  >
                    <div className="flex items-center gap-1.5 mb-1 font-semibold text-slate-200">
                      <Icon className="w-3.5 h-3.5 text-indigo-400" />
                      <span>{cov.title}</span>
                    </div>
                    <p className="text-[10px] text-slate-400">{cov.desc}</p>
                  </div>
                );
              })}
            </div>
          </div>

          {/* Test Execution Stream on Selected Device */}
          <div className="space-y-2 pt-2 border-t border-slate-800">
            <div className="flex items-center justify-between font-mono text-[11px] text-slate-400">
              <span className="font-bold text-slate-300 uppercase">Recent Non-UI Executions</span>
              <span>Zero UI Overhead</span>
            </div>

            <div className="space-y-2">
              {testExecutions.map((exec) => (
                <div key={exec.id} className="p-2.5 rounded bg-[#020617] border border-slate-800 space-y-1.5">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-indigo-400 font-bold text-[11px]">{exec.id}</span>
                      <span className="text-slate-200 font-semibold text-xs">{exec.title}</span>
                    </div>
                    <StatusBadge status={exec.status} size="sm" />
                  </div>

                  <div className="text-[11px] font-mono text-slate-400 bg-[#0B1120] p-1.5 rounded border border-slate-800/80">
                    &gt; {exec.logSummary}
                  </div>

                  <div className="flex items-center justify-between text-[10px] font-mono text-slate-500">
                    <div className="flex items-center gap-3">
                      <span>CPU Peak: <strong className="text-slate-300">{exec.deviceMetrics.cpuPeakPct}%</strong></span>
                      <span>RAM: <strong className="text-slate-300">{exec.deviceMetrics.ramUsedMb}MB</strong></span>
                      <span>Drain: <strong className="text-slate-300">{exec.deviceMetrics.batteryDrainPct}%</strong></span>
                    </div>
                    <span>{exec.durationMs}ms</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

import React, { useState, useEffect } from "react";
import {
  Smartphone,
  Tablet,
  Play,
  RotateCw,
  Wifi,
  WifiOff,
  Radio,
  Fingerprint,
  Camera,
  MapPin,
  Bell,
  RefreshCw,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  Terminal,
  Activity,
  Zap,
  Sliders,
  Layers,
  Cpu,
  Eye,
  Send,
  Sparkles,
  Download,
  Copy,
  Check
} from "lucide-react";

interface DeviceItem {
  id: string;
  name: string;
  platform: "iOS" | "Android";
  osVersion: string;
  screenSize: string;
  status: "idle" | "running" | "ready" | "busy";
  battery: number;
  temperature: string;
  memoryMb: number;
  ipAddress: string;
  network: "WiFi" | "5G" | "4G" | "3G" | "Offline" | "Flaky";
}

const initialDevices: DeviceItem[] = [
  {
    id: "dev-ios-15pm",
    name: "iPhone 15 Pro Max",
    platform: "iOS",
    osVersion: "iOS 17.5",
    screenSize: "430 x 932 pt (@3x)",
    status: "running",
    battery: 94,
    temperature: "31.2°C",
    memoryMb: 1420,
    ipAddress: "192.168.1.104",
    network: "5G"
  },
  {
    id: "dev-and-p8p",
    name: "Google Pixel 8 Pro",
    platform: "Android",
    osVersion: "Android 14 (API 34)",
    screenSize: "412 x 892 dp (@3.5x)",
    status: "ready",
    battery: 88,
    temperature: "29.8°C",
    memoryMb: 2150,
    ipAddress: "192.168.1.112",
    network: "WiFi"
  },
  {
    id: "dev-and-s24u",
    name: "Samsung Galaxy S24 Ultra",
    platform: "Android",
    osVersion: "One UI 6.1 (Android 14)",
    screenSize: "384 x 824 dp (@4x)",
    status: "idle",
    battery: 76,
    temperature: "30.5°C",
    memoryMb: 2840,
    ipAddress: "192.168.1.118",
    network: "4G"
  },
  {
    id: "dev-ios-ipad",
    name: "iPad Pro 12.9-inch (M2)",
    platform: "iOS",
    osVersion: "iPadOS 17.5",
    screenSize: "1024 x 1366 pt (@2x)",
    status: "idle",
    battery: 99,
    temperature: "27.4°C",
    memoryMb: 1890,
    ipAddress: "192.168.1.109",
    network: "WiFi"
  }
];

const mobileSuites = [
  {
    id: "mob-1",
    name: "Biometric FaceID & Secure Keyring Unlock",
    framework: "Appium 2.0 / XCUITest",
    status: "Passed",
    duration: "1.4s",
    assertions: "14/14",
    lastRun: "2 mins ago"
  },
  {
    id: "mob-2",
    name: "Native Payment Sheet (Apple Pay & GPay Token)",
    framework: "Detox Native Runner",
    status: "Passed",
    duration: "2.8s",
    assertions: "22/22",
    lastRun: "5 mins ago"
  },
  {
    id: "mob-3",
    name: "Push Notification Wake & Deep-link Routing",
    framework: "Maestro Mobile",
    status: "Passed",
    duration: "950ms",
    assertions: "8/8",
    lastRun: "12 mins ago"
  },
  {
    id: "mob-4",
    name: "Offline SQLite State Sync on Reconnection",
    framework: "Appium Espresso",
    status: "Warning",
    duration: "3.9s",
    assertions: "18/20",
    lastRun: "20 mins ago"
  },
  {
    id: "mob-5",
    name: "Camera Barcode Scanner & Real-time OCR",
    framework: "XCUITest / VisionKit",
    status: "Passed",
    duration: "1.1s",
    assertions: "12/12",
    lastRun: "35 mins ago"
  }
];

export const MobileTestingView: React.FC = () => {
  const [devices, setDevices] = useState<DeviceItem[]>(initialDevices);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string>("dev-ios-15pm");
  const [activeFramework, setActiveFramework] = useState<string>("Appium 2.0");
  const [orientation, setOrientation] = useState<"portrait" | "landscape">("portrait");
  const [networkCondition, setNetworkCondition] = useState<string>("5G");
  const [deepLinkInput, setDeepLinkInput] = useState<string>("aiqa://checkout/confirmation?order_id=ord_99812&auth_ref=token_secops");
  const [deepLinkStatus, setDeepLinkStatus] = useState<string | null>(null);
  const [isBiometricPromptActive, setIsBiometricPromptActive] = useState<boolean>(false);
  const [screenInteractionLog, setScreenInteractionLog] = useState<string>("Device ready. Touch stream active.");
  const [isRunningAll, setIsRunningAll] = useState<boolean>(false);
  const [copiedLog, setCopiedLog] = useState<boolean>(false);
  const [mockAppScreen, setMockAppScreen] = useState<"home" | "checkout" | "auth" | "camera">("home");

  const activeDevice = devices.find((d) => d.id === selectedDeviceId) || devices[0];

  const [logs, setLogs] = useState<Array<{ id: string; time: string; level: string; tag: string; message: string }>>([
    { id: "1", time: "10:04:12.102", level: "INFO", tag: "AppiumDriver", message: "Session established with capabilities: platformName=iOS, automationName=XCUITest" },
    { id: "2", time: "10:04:13.419", level: "DEBUG", tag: "Accessibility", message: "Hit test identified element @xpath='//XCUIElementTypeButton[@name=\"Biometric Auth\"]'" },
    { id: "3", time: "10:04:14.004", level: "INFO", tag: "LocalAuth", message: "LAContext evaluatePolicy:deviceOwnerAuthenticationWithBiometrics responded SUCCESS" },
    { id: "4", time: "10:04:15.228", level: "DEBUG", tag: "NetworkStack", message: "Injected network condition: 5G Latency 18ms, Bandwidth 120Mbps down" },
    { id: "5", time: "10:04:16.890", level: "INFO", tag: "ViewHierarchy", message: "Screen rendered with 0 frame drops (60 FPS fluid rendering)" }
  ]);

  const handleLaunchDeepLink = () => {
    setDeepLinkStatus("Dispatching deep link to application bundle...");
    setTimeout(() => {
      setMockAppScreen("checkout");
      setDeepLinkStatus("Deep link resolved successfully: Routed to Checkout confirmation view.");
      setLogs((prev) => [
        {
          id: String(Date.now()),
          time: new Date().toLocaleTimeString(),
          level: "INFO",
          tag: "DeepLinkHandler",
          message: `Received URI payload: ${deepLinkInput}`
        },
        ...prev
      ]);
    }, 600);
  };

  const handleSimulateBiometrics = (success: boolean) => {
    setIsBiometricPromptActive(true);
    setScreenInteractionLog("Biometric sensor prompt triggered (FaceID / Fingerprint)...");
    setTimeout(() => {
      setIsBiometricPromptActive(false);
      if (success) {
        setScreenInteractionLog("Biometrics verified: Local authentication token granted.");
        setMockAppScreen("auth");
      } else {
        setScreenInteractionLog("Biometrics failed: Access denied (LAErrorUserFallback).");
      }
      setLogs((prev) => [
        {
          id: String(Date.now()),
          time: new Date().toLocaleTimeString(),
          level: success ? "INFO" : "WARN",
          tag: "BiometricSensor",
          message: success
            ? "Simulated FaceID Match: Cryptographic challenge signed."
            : "Simulated FaceID Mismatch: LAErrorBiometryNotEnrolled / rejected."
        },
        ...prev
      ]);
    }, 900);
  };

  const handleRunAllSuites = () => {
    setIsRunningAll(true);
    setTimeout(() => {
      setIsRunningAll(false);
      setLogs((prev) => [
        {
          id: String(Date.now()),
          time: new Date().toLocaleTimeString(),
          level: "INFO",
          tag: "SuiteRunner",
          message: "All 5 mobile test suites executed across device cluster: 5/5 PASSED (Total: 4.8s)"
        },
        ...prev
      ]);
    }, 1500);
  };

  const handleCopyLogs = () => {
    const text = logs.map((l) => `[${l.time}] [${l.level}] [${l.tag}]: ${l.message}`).join("\n");
    navigator.clipboard.writeText(text);
    setCopiedLog(true);
    setTimeout(() => setCopiedLog(false), 2000);
  };

  return (
    <div id="mobile-testing-hub" className="space-y-3.5 font-sans">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 bg-[#0D0D0E] border border-slate-800 rounded p-3.5">
        <div className="flex items-center gap-2.5">
          <div className="w-8 h-8 rounded bg-indigo-600/20 border border-indigo-500/40 flex items-center justify-center text-indigo-400">
            <Smartphone className="w-4 h-4" />
          </div>
          <div>
            <h1 className="text-sm font-bold font-mono uppercase text-white tracking-wider flex items-center gap-2">
              Mobile Test Automation &amp; Device Farm
              <span className="text-[9px] px-1.5 py-0.2 rounded bg-indigo-500/10 text-indigo-400 border border-indigo-500/30 font-mono">
                Appium 2.0 + Maestro
              </span>
            </h1>
            <p className="text-[10px] text-slate-400 font-mono mt-0.5">
              Real iOS simulators, Android emulators, native biometric injection, and deep-link verification
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {/* Framework Switcher */}
          <div className="flex bg-slate-900 border border-slate-800 rounded overflow-hidden text-[10px] font-mono">
            {["Appium 2.0", "Maestro", "Detox", "XCUITest"].map((fw) => (
              <button
                key={fw}
                onClick={() => setActiveFramework(fw)}
                className={`px-2 py-1 transition-colors ${
                  activeFramework === fw
                    ? "bg-indigo-600 text-white font-bold"
                    : "text-slate-400 hover:text-slate-200"
                }`}
              >
                {fw}
              </button>
            ))}
          </div>

          <button
            onClick={handleRunAllSuites}
            disabled={isRunningAll}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white text-[11px] font-mono font-bold shadow-sm transition-all"
          >
            {isRunningAll ? (
              <RefreshCw className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <Play className="w-3.5 h-3.5 fill-current" />
            )}
            <span>{isRunningAll ? "Executing Farm..." : "Run All Mobile Suites"}</span>
          </button>
        </div>
      </div>

      {/* Top 4 KPI Metrics */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs font-mono">
        <div className="bg-[#0D0D0E] border border-slate-800 rounded p-2.5">
          <div className="flex items-center justify-between text-slate-400 text-[10px]">
            <span>Connected Devices</span>
            <Smartphone className="w-3 h-3 text-indigo-400" />
          </div>
          <div className="text-base font-bold text-white mt-1">4 Online</div>
          <div className="text-[9px] text-emerald-400 mt-0.5">● 2 iOS Simulators &bull; 2 Android Emulators</div>
        </div>

        <div className="bg-[#0D0D0E] border border-slate-800 rounded p-2.5">
          <div className="flex items-center justify-between text-slate-400 text-[10px]">
            <span>Mobile Test Pass Rate</span>
            <CheckCircle2 className="w-3 h-3 text-emerald-400" />
          </div>
          <div className="text-base font-bold text-emerald-400 mt-1">98.2%</div>
          <div className="text-[9px] text-slate-400 mt-0.5">72 assertions verified today</div>
        </div>

        <div className="bg-[#0D0D0E] border border-slate-800 rounded p-2.5">
          <div className="flex items-center justify-between text-slate-400 text-[10px]">
            <span>Touch Interaction Latency</span>
            <Zap className="w-3 h-3 text-amber-400" />
          </div>
          <div className="text-base font-bold text-slate-200 mt-1">38 ms</div>
          <div className="text-[9px] text-slate-400 mt-0.5">Fast touch sampling rate (120Hz)</div>
        </div>

        <div className="bg-[#0D0D0E] border border-slate-800 rounded p-2.5">
          <div className="flex items-center justify-between text-slate-400 text-[10px]">
            <span>Jank / Frame Drop Rate</span>
            <Activity className="w-3 h-3 text-cyan-400" />
          </div>
          <div className="text-base font-bold text-slate-200 mt-1">&lt; 0.2%</div>
          <div className="text-[9px] text-emerald-400 mt-0.5">Consistent 60 FPS viewport</div>
        </div>
      </div>

      {/* Main Grid: Left (Device Fleet & Hardware Controls) + Right (Live Simulated Device View & Touch Stream) */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-3">
        {/* Left Column: Device Selector & Hardware Conditioners (5 cols) */}
        <div className="lg:col-span-5 space-y-3">
          {/* Device Fleet Selector */}
          <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3 space-y-2.5">
            <div className="flex items-center justify-between pb-1.5 border-b border-slate-800">
              <span className="text-[10px] font-bold font-mono uppercase text-white tracking-wider">
                Target Device Farm Fleet
              </span>
              <span className="text-[9px] font-mono text-slate-400">Click to attach stream</span>
            </div>

            <div className="space-y-1.5">
              {devices.map((d) => {
                const isSelected = selectedDeviceId === d.id;
                return (
                  <div
                    key={d.id}
                    onClick={() => setSelectedDeviceId(d.id)}
                    className={`p-2 rounded border cursor-pointer transition-all text-xs font-mono ${
                      isSelected
                        ? "bg-indigo-600/15 border-indigo-500/40 text-white"
                        : "bg-[#070708] border-slate-800 hover:border-slate-700 text-slate-300"
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        {d.platform === "iOS" ? (
                          <span className="text-[9px] px-1.5 py-0.2 rounded bg-slate-800 text-slate-200 font-bold">iOS</span>
                        ) : (
                          <span className="text-[9px] px-1.5 py-0.2 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 font-bold">Android</span>
                        )}
                        <span className="font-bold text-[11px]">{d.name}</span>
                      </div>
                      <span
                        className={`text-[9px] px-1.5 py-0.2 rounded uppercase ${
                          d.status === "running"
                            ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/30"
                            : "bg-slate-800 text-slate-400"
                        }`}
                      >
                        {d.status}
                      </span>
                    </div>

                    <div className="grid grid-cols-3 gap-1 text-[9px] text-slate-400 mt-1.5 pt-1 border-t border-slate-800/60">
                      <div>OS: <span className="text-slate-200">{d.osVersion}</span></div>
                      <div>RAM: <span className="text-slate-200">{d.memoryMb} MB</span></div>
                      <div>Batt: <span className="text-emerald-400">{d.battery}%</span></div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {/* Network & Hardware Conditioners */}
          <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3 space-y-2.5">
            <div className="flex items-center justify-between pb-1.5 border-b border-slate-800">
              <span className="text-[10px] font-bold font-mono uppercase text-white tracking-wider flex items-center gap-1.5">
                <Sliders className="w-3 h-3 text-indigo-400" />
                Hardware &amp; Network Conditioners
              </span>
            </div>

            <div className="space-y-2 text-xs font-mono">
              {/* Network Profiler */}
              <div>
                <label className="text-[9px] uppercase text-slate-400 block mb-1">
                  Network Link Conditioner
                </label>
                <div className="grid grid-cols-3 gap-1 text-[10px]">
                  {["5G", "4G", "WiFi", "3G", "Flaky", "Offline"].map((net) => (
                    <button
                      key={net}
                      onClick={() => {
                        setNetworkCondition(net);
                        setLogs((prev) => [
                          {
                            id: String(Date.now()),
                            time: new Date().toLocaleTimeString(),
                            level: "INFO",
                            tag: "NetworkConditioner",
                            message: `Attached network simulation profile: ${net}`
                          },
                          ...prev
                        ]);
                      }}
                      className={`p-1.5 rounded border text-center font-mono ${
                        networkCondition === net
                          ? "bg-indigo-600/20 border-indigo-500/40 text-indigo-300 font-bold"
                          : "bg-[#070708] border-slate-800 text-slate-400 hover:text-slate-200"
                      }`}
                    >
                      {net === "Offline" ? <WifiOff className="w-2.5 h-2.5 inline mr-1" /> : <Wifi className="w-2.5 h-2.5 inline mr-1" />}
                      {net}
                    </button>
                  ))}
                </div>
              </div>

              {/* Hardware Actions */}
              <div className="pt-1.5 border-t border-slate-800 space-y-1.5">
                <label className="text-[9px] uppercase text-slate-400 block">
                  Hardware Sensor Triggers
                </label>
                <div className="grid grid-cols-2 gap-1.5 text-[10px]">
                  <button
                    onClick={() => handleSimulateBiometrics(true)}
                    className="p-1.5 rounded bg-[#070708] border border-slate-800 hover:border-indigo-500/40 text-slate-200 flex items-center justify-center gap-1"
                  >
                    <Fingerprint className="w-3 h-3 text-indigo-400" />
                    <span>Pass Biometrics</span>
                  </button>
                  <button
                    onClick={() => handleSimulateBiometrics(false)}
                    className="p-1.5 rounded bg-[#070708] border border-slate-800 hover:border-rose-500/40 text-slate-200 flex items-center justify-center gap-1"
                  >
                    <Fingerprint className="w-3 h-3 text-rose-400" />
                    <span>Fail Biometrics</span>
                  </button>
                  <button
                    onClick={() => {
                      setOrientation((prev) => (prev === "portrait" ? "landscape" : "portrait"));
                      setLogs((prev) => [
                        {
                          id: String(Date.now()),
                          time: new Date().toLocaleTimeString(),
                          level: "INFO",
                          tag: "DisplayManager",
                          message: `Screen orientation changed to ${orientation === "portrait" ? "LANDSCAPE" : "PORTRAIT"}`
                        },
                        ...prev
                      ]);
                    }}
                    className="p-1.5 rounded bg-[#070708] border border-slate-800 hover:border-slate-700 text-slate-200 flex items-center justify-center gap-1"
                  >
                    <RotateCw className="w-3 h-3 text-slate-400" />
                    <span>Rotate ({orientation})</span>
                  </button>
                  <button
                    onClick={() => {
                      setMockAppScreen("camera");
                      setLogs((prev) => [
                        {
                          id: String(Date.now()),
                          time: new Date().toLocaleTimeString(),
                          level: "INFO",
                          tag: "CameraMock",
                          message: "Injected synthetic test QR code barcode video frame into AVCaptureSession."
                        },
                        ...prev
                      ]);
                    }}
                    className="p-1.5 rounded bg-[#070708] border border-slate-800 hover:border-slate-700 text-slate-200 flex items-center justify-center gap-1"
                  >
                    <Camera className="w-3 h-3 text-slate-400" />
                    <span>Mock Camera QR</span>
                  </button>
                </div>
              </div>

              {/* Deep Link Injector */}
              <div className="pt-1.5 border-t border-slate-800 space-y-1">
                <label className="text-[9px] uppercase text-slate-400 block">
                  Deep-Link URI Dispatcher
                </label>
                <div className="flex gap-1.5">
                  <input
                    type="text"
                    value={deepLinkInput}
                    onChange={(e) => setDeepLinkInput(e.target.value)}
                    className="flex-1 p-1.5 rounded bg-[#070708] border border-slate-800 text-[10px] font-mono text-slate-200 focus:outline-none focus:border-indigo-500"
                  />
                  <button
                    onClick={handleLaunchDeepLink}
                    className="px-2.5 py-1 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-bold text-[10px] flex items-center gap-1"
                  >
                    <Send className="w-2.5 h-2.5" />
                    <span>Launch</span>
                  </button>
                </div>
                {deepLinkStatus && (
                  <p className="text-[9px] text-emerald-400 pt-0.5">{deepLinkStatus}</p>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* Right Column: Live Device Canvas & Interactive Virtual Phone (7 cols) */}
        <div className="lg:col-span-7 space-y-3">
          {/* Virtual Phone Canvas Preview */}
          <div className="bg-[#0D0D0E] border border-slate-800 rounded p-3.5 flex flex-col items-center justify-center">
            <div className="w-full flex items-center justify-between pb-2 mb-2 border-b border-slate-800 text-xs font-mono">
              <div className="flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
                <span className="font-bold text-white">{activeDevice.name}</span>
                <span className="text-[10px] text-slate-400">({activeDevice.screenSize})</span>
              </div>
              <div className="flex items-center gap-2 text-[10px] text-slate-400">
                <span>FPS: <strong className="text-emerald-400">60.0</strong></span>
                <span>&bull;</span>
                <span>Net: <strong className="text-indigo-400">{networkCondition}</strong></span>
              </div>
            </div>

            {/* Phone Frame Mockup */}
            <div
              className={`bg-[#050507] border-4 border-slate-700 rounded-[28px] p-2.5 shadow-2xl relative transition-all duration-300 ${
                orientation === "landscape" ? "w-[460px] h-[240px]" : "w-[260px] h-[440px]"
              }`}
            >
              {/* Dynamic Island / Camera notch */}
              <div className="w-20 h-4 bg-slate-900 rounded-full mx-auto mb-1.5 flex items-center justify-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-slate-700" />
                <span className="w-1.5 h-1.5 rounded-full bg-indigo-500/60" />
              </div>

              {/* Virtual Screen View Content */}
              <div className="bg-[#0c101b] rounded-2xl w-full h-[calc(100%-28px)] overflow-hidden border border-slate-800 flex flex-col justify-between p-2.5 text-xs select-none">
                {/* Virtual App Bar */}
                <div className="flex items-center justify-between border-b border-slate-800/80 pb-1.5 text-[10px] font-mono">
                  <span className="font-bold text-indigo-400 flex items-center gap-1">
                    <Sparkles className="w-3 h-3" /> Apex Mobile
                  </span>
                  <span className="text-[8px] px-1 py-0.2 rounded bg-indigo-950 text-indigo-300 font-mono">
                    v2.4.1
                  </span>
                </div>

                {/* Simulated Screen Content based on state */}
                <div className="flex-1 flex flex-col justify-center items-center text-center p-2 space-y-2">
                  {isBiometricPromptActive ? (
                    <div className="p-3 bg-slate-900/90 border border-indigo-500/50 rounded-xl space-y-2 animate-in zoom-in-95">
                      <Fingerprint className="w-8 h-8 text-indigo-400 mx-auto animate-pulse" />
                      <p className="text-[10px] font-mono text-slate-200">Confirm with Face ID to Authorize Payment</p>
                    </div>
                  ) : mockAppScreen === "checkout" ? (
                    <div className="space-y-1.5 w-full">
                      <div className="text-[10px] font-mono text-emerald-400 font-bold">✓ Order #99812 Loaded</div>
                      <div className="p-2 rounded bg-slate-900 border border-slate-800 text-[9px] text-slate-300 space-y-1">
                        <div>Total: $149.00 USD</div>
                        <div>Recipient: SecOps Auth Cluster</div>
                      </div>
                      <button
                        onClick={() => {
                          setMockAppScreen("auth");
                          setScreenInteractionLog("Tapped 'Confirm Payment' via simulated touch.");
                        }}
                        className="w-full py-1.5 rounded bg-emerald-600 text-white font-bold text-[10px]"
                      >
                        Confirm Payment
                      </button>
                    </div>
                  ) : mockAppScreen === "camera" ? (
                    <div className="space-y-2 w-full">
                      <div className="w-20 h-20 border-2 border-dashed border-indigo-400 mx-auto flex items-center justify-center rounded bg-indigo-950/40">
                        <Camera className="w-6 h-6 text-indigo-400" />
                      </div>
                      <p className="text-[9px] text-slate-300 font-mono">Scanning QR payload...</p>
                      <button
                        onClick={() => {
                          setMockAppScreen("home");
                          setScreenInteractionLog("QR code payload captured: token_alpha_921.");
                        }}
                        className="px-2 py-1 rounded bg-slate-800 text-slate-200 text-[9px]"
                      >
                        Return to Home
                      </button>
                    </div>
                  ) : mockAppScreen === "auth" ? (
                    <div className="space-y-2 w-full">
                      <CheckCircle2 className="w-8 h-8 text-emerald-400 mx-auto" />
                      <div className="text-[10px] font-bold text-white">Authenticated</div>
                      <p className="text-[9px] text-slate-400">Secure enclave session established</p>
                      <button
                        onClick={() => setMockAppScreen("home")}
                        className="px-3 py-1 rounded bg-slate-800 text-slate-300 text-[9px]"
                      >
                        Back to Home
                      </button>
                    </div>
                  ) : (
                    <div className="space-y-2 w-full">
                      <div className="w-10 h-10 rounded-full bg-indigo-600/20 border border-indigo-500/40 mx-auto flex items-center justify-center text-indigo-400">
                        <Smartphone className="w-5 h-5" />
                      </div>
                      <div className="font-bold text-slate-200 text-xs">Payment Agent Ready</div>
                      <div className="space-y-1">
                        <button
                          onClick={() => {
                            setMockAppScreen("checkout");
                            setScreenInteractionLog("Tapped 'Initiate Checkout' button.");
                          }}
                          className="w-full py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-mono text-[10px] font-bold"
                        >
                          Initiate Checkout
                        </button>
                        <button
                          onClick={() => handleSimulateBiometrics(true)}
                          className="w-full py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 font-mono text-[9px]"
                        >
                          Test FaceID Auth
                        </button>
                      </div>
                    </div>
                  )}
                </div>

                {/* Touch Feedback Bar */}
                <div className="p-1 rounded bg-black/60 border border-slate-800 text-[8px] font-mono text-slate-400 truncate">
                  {screenInteractionLog}
                </div>
              </div>
            </div>

            {/* Quick Virtual Gesture Bar */}
            <div className="flex items-center gap-1.5 mt-3 text-[10px] font-mono text-slate-400">
              <span className="text-[9px] uppercase">Gesture:</span>
              <button
                onClick={() => setScreenInteractionLog("Dispatched swipe-up gesture.")}
                className="px-2 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300"
              >
                Swipe Up
              </button>
              <button
                onClick={() => setScreenInteractionLog("Dispatched pull-to-refresh gesture.")}
                className="px-2 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300"
              >
                Pull-to-Refresh
              </button>
              <button
                onClick={() => {
                  setMockAppScreen("home");
                  setScreenInteractionLog("Dispatched Home button keycode.");
                }}
                className="px-2 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300"
              >
                Home
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* Bottom Grid: Mobile Test Scenarios & Real-Time Logcat/Console Stream */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-3">
        {/* Scenarios Table (6 cols) */}
        <div className="lg:col-span-6 bg-[#0D0D0E] border border-slate-800 rounded p-3 space-y-2">
          <div className="flex items-center justify-between pb-1.5 border-b border-slate-800">
            <span className="text-[10px] font-bold font-mono uppercase text-white tracking-wider">
              Automated Mobile Scenarios ({mobileSuites.length})
            </span>
            <span className="text-[9px] font-mono text-slate-400">Framework: {activeFramework}</span>
          </div>

          <div className="divide-y divide-slate-800/60 font-mono text-xs">
            {mobileSuites.map((s) => (
              <div key={s.id} className="py-2 flex items-center justify-between gap-2">
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5">
                    <span
                      className={`text-[8px] px-1 py-0.2 rounded font-bold uppercase ${
                        s.status === "Passed"
                          ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/30"
                          : "bg-amber-500/10 text-amber-400 border border-amber-500/30"
                      }`}
                    >
                      {s.status}
                    </span>
                    <span className="font-bold text-slate-200 text-[11px] truncate">{s.name}</span>
                  </div>
                  <div className="text-[9px] text-slate-400 mt-0.5 flex items-center gap-2">
                    <span>{s.framework}</span>
                    <span>&bull;</span>
                    <span>Assertions: {s.assertions}</span>
                    <span>&bull;</span>
                    <span>Duration: {s.duration}</span>
                  </div>
                </div>

                <button
                  onClick={() => {
                    setLogs((prev) => [
                      {
                        id: String(Date.now()),
                        time: new Date().toLocaleTimeString(),
                        level: "INFO",
                        tag: "TestRunner",
                        message: `Manually executed: "${s.name}" on ${activeDevice.name} &bull; PASSED (${s.duration})`
                      },
                      ...prev
                    ]);
                  }}
                  className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-200 text-[10px] shrink-0"
                >
                  Run
                </button>
              </div>
            ))}
          </div>
        </div>

        {/* Live Device Logcat Stream (6 cols) */}
        <div className="lg:col-span-6 bg-[#0D0D0E] border border-slate-800 rounded p-3 space-y-2 flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between pb-1.5 border-b border-slate-800">
              <span className="text-[10px] font-bold font-mono uppercase text-white tracking-wider flex items-center gap-1.5">
                <Terminal className="w-3 h-3 text-indigo-400" />
                Live Device Syslog &amp; Logcat Stream
              </span>
              <button
                onClick={handleCopyLogs}
                className="flex items-center gap-1 text-[10px] text-indigo-400 hover:text-indigo-300 font-mono"
              >
                {copiedLog ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
                <span>{copiedLog ? "Copied" : "Copy Logs"}</span>
              </button>
            </div>

            <div className="mt-2 bg-[#070708] border border-slate-800 rounded p-2.5 max-h-52 overflow-y-auto space-y-1.5 font-mono text-[10px]">
              {logs.map((log) => (
                <div key={log.id} className="leading-relaxed flex items-start gap-1.5">
                  <span className="text-slate-500 shrink-0">{log.time}</span>
                  <span
                    className={`font-bold px-1 py-0.2 rounded text-[8px] shrink-0 ${
                      log.level === "ERROR"
                        ? "bg-rose-500/10 text-rose-400 border border-rose-500/30"
                        : log.level === "WARN"
                        ? "bg-amber-500/10 text-amber-400 border border-amber-500/30"
                        : log.level === "DEBUG"
                        ? "bg-cyan-500/10 text-cyan-400 border border-cyan-500/30"
                        : "bg-indigo-500/10 text-indigo-400 border border-indigo-500/30"
                    }`}
                  >
                    {log.level}
                  </span>
                  <span className="text-slate-400 font-bold shrink-0">[{log.tag}]:</span>
                  <span className="text-slate-300 break-all">{log.message}</span>
                </div>
              ))}
            </div>
          </div>

          <div className="pt-2 border-t border-slate-800 flex items-center justify-between text-[10px] font-mono text-slate-400">
            <span>ADB / libimobiledevice connected</span>
            <span className="text-emerald-400">● 100% Stream Sync</span>
          </div>
        </div>
      </div>
    </div>
  );
};

import { EndpointFailureAlertConfig, EndpointFailureIncident } from "../types";

// In-memory failure tracking
const failureCounter: Record<string, { count: number; lastError: string; lastTime: string; name: string }> = {};

export function checkNotificationSupport(): boolean {
  return typeof window !== "undefined" && "Notification" in window;
}

export function getNotificationPermission(): NotificationPermission {
  if (!checkNotificationSupport()) return "denied";
  return Notification.permission;
}

export async function requestNotificationPermission(): Promise<NotificationPermission> {
  if (!checkNotificationSupport()) return "denied";
  try {
    const permission = await Notification.requestPermission();
    return permission;
  } catch (err) {
    console.error("Failed to request notification permission:", err);
    return "denied";
  }
}

export function playAlertChime() {
  if (typeof window === "undefined") return;
  try {
    const AudioContextClass = window.AudioContext || (window as any).webkitAudioContext;
    if (!AudioContextClass) return;
    const ctx = new AudioContextClass();
    
    // First high beep
    const osc1 = ctx.createOscillator();
    const gain1 = ctx.createGain();
    osc1.type = "sine";
    osc1.frequency.setValueAtTime(880, ctx.currentTime); // A5
    osc1.frequency.exponentialRampToValueAtTime(440, ctx.currentTime + 0.15);
    gain1.gain.setValueAtTime(0.3, ctx.currentTime);
    gain1.gain.exponentialRampToValueAtTime(0.01, ctx.currentTime + 0.15);
    osc1.connect(gain1);
    gain1.connect(ctx.destination);
    osc1.start(ctx.currentTime);
    osc1.stop(ctx.currentTime + 0.15);

    // Second warning tone
    const osc2 = ctx.createOscillator();
    const gain2 = ctx.createGain();
    osc2.type = "triangle";
    osc2.frequency.setValueAtTime(587.33, ctx.currentTime + 0.18); // D5
    osc2.frequency.exponentialRampToValueAtTime(329.63, ctx.currentTime + 0.35); // E4
    gain2.gain.setValueAtTime(0.25, ctx.currentTime + 0.18);
    gain2.gain.exponentialRampToValueAtTime(0.01, ctx.currentTime + 0.35);
    osc2.connect(gain2);
    gain2.connect(ctx.destination);
    osc2.start(ctx.currentTime + 0.18);
    osc2.stop(ctx.currentTime + 0.35);
  } catch (err) {
    console.warn("Could not play audio chime:", err);
  }
}

export function sendBrowserNotification(
  title: string,
  options?: NotificationOptions & { playSound?: boolean }
): Notification | null {
  if (!checkNotificationSupport()) {
    console.warn("Notifications not supported in this environment");
    return null;
  }

  if (Notification.permission !== "granted") {
    console.warn("Notification permission is not granted (current:", Notification.permission, ")");
    return null;
  }

  try {
    if (options?.playSound) {
      playAlertChime();
    }

    const notification = new Notification(title, {
      icon: "/favicon.ico",
      badge: "/favicon.ico",
      tag: "ai-brain-endpoint-outage",
      requireInteraction: true,
      ...options
    });

    notification.onclick = () => {
      window.focus();
      notification.close();
    };

    return notification;
  } catch (err) {
    console.error("Failed to construct Notification:", err);
    return null;
  }
}

/**
 * Record a failure on an AI model or API endpoint.
 * If consecutive failures >= threshold and alerts are enabled, fires browser notification.
 */
export function recordEndpointFailure(
  endpointId: string,
  endpointName: string,
  endpointType: "local_ollama" | "remote_api" | "custom_ingress",
  errorMessage: string,
  config: EndpointFailureAlertConfig,
  onIncidentTriggered?: (incident: EndpointFailureIncident) => void
): { count: number; alerted: boolean } {
  const current = failureCounter[endpointId] || { count: 0, lastError: "", lastTime: "", name: endpointName };
  const newCount = current.count + 1;
  const now = new Date().toISOString();

  failureCounter[endpointId] = {
    count: newCount,
    lastError: errorMessage,
    lastTime: now,
    name: endpointName
  };

  let alerted = false;

  const isMonitored =
    config.monitoredEndpoints.includes("all") ||
    config.monitoredEndpoints.includes(endpointId);

  if (config.enabled && isMonitored && newCount >= config.failureThreshold) {
    const title = `⚠️ AI Endpoint Failure Alert: ${endpointName}`;
    const body = `Consecutive Failures: ${newCount} (Threshold: ${config.failureThreshold})\nError: ${errorMessage.slice(0, 140)}\nRecommendation: Failover or verify endpoint credentials.`;

    sendBrowserNotification(title, {
      body,
      playSound: config.soundEnabled
    });

    alerted = true;

    if (onIncidentTriggered) {
      onIncidentTriggered({
        id: `inc-${Date.now().toString().slice(-6)}`,
        endpointId,
        endpointName,
        endpointType,
        consecutiveFailures: newCount,
        timestamp: now,
        lastErrorMessage: errorMessage,
        notificationSent: true
      });
    }
  }

  return { count: newCount, alerted };
}

/**
 * Reset failure count for an endpoint when a successful inference or connection occurs
 */
export function resetEndpointFailures(endpointId: string) {
  if (failureCounter[endpointId]) {
    delete failureCounter[endpointId];
  }
}

/**
 * Get all current consecutive failure counts
 */
export function getEndpointFailureCounts(): Record<string, { count: number; lastError: string; lastTime: string; name: string }> {
  return { ...failureCounter };
}

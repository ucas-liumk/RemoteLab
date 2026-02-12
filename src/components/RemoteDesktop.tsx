import { useEffect, useRef, useState, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  X,
  Maximize2,
  Minimize2,
  Loader2,
  Cpu,
  RefreshCw,
  CheckCircle2,
  Circle,
  AlertCircle,
  Monitor,
  Download,
  Server,
  Wifi,
} from "lucide-react";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import * as api from "../services/api";
import { VncScreen, type VncScreenHandle } from "react-vnc";

interface RemoteDesktopProps {
  deviceName: string;
  host: string;
  user: string;
  port?: number;
  onClose: () => void;
}

// ======================== Setup Steps ========================

interface SetupStep {
  id: string;
  label: string;
  icon: React.ElementType;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
}

const NVENC_STEPS: SetupStep[] = [
  { id: "gpu", label: "Detect GPU", icon: Cpu, status: "pending" },
  { id: "xorg", label: "Install display server", icon: Monitor, status: "pending" },
  { id: "desktop", label: "Install XFCE desktop", icon: Server, status: "pending" },
  { id: "sunshine", label: "Download & install Sunshine", icon: Download, status: "pending" },
  { id: "display", label: "Start virtual display", icon: Monitor, status: "pending" },
  { id: "start", label: "Start Sunshine", icon: Server, status: "pending" },
  { id: "connect", label: "Connect", icon: Wifi, status: "pending" },
];

const VNC_STEPS: SetupStep[] = [
  { id: "gpu", label: "Detect GPU", icon: Cpu, status: "pending" },
  { id: "vnc_install", label: "Install VNC server", icon: Download, status: "pending" },
  { id: "vnc_start", label: "Start VNC server", icon: Server, status: "pending" },
  { id: "tunnel", label: "Create secure tunnel", icon: Wifi, status: "pending" },
  { id: "connect", label: "Connect", icon: Monitor, status: "pending" },
];

function mapProgressToSteps(
  steps: SetupStep[],
  phase: string,
  percent: number,
  message: string,
): SetupStep[] {
  const updated = steps.map((s) => ({ ...s }));

  if (phase === "gpu_detect") {
    const gpu = updated.find((s) => s.id === "gpu")!;
    if (percent >= 5) {
      gpu.status = "done";
      gpu.detail = message;
    } else {
      gpu.status = "active";
    }
  } else if (phase === "sunshine_setup") {
    const gpu = updated.find((s) => s.id === "gpu");
    if (gpu) gpu.status = "done";

    if (percent <= 10) {
      setStepActive(updated, "xorg", message);
    } else if (percent <= 30) {
      setStepDone(updated, "xorg");
      setStepActive(updated, "desktop", message);
    } else if (percent <= 40) {
      setStepDone(updated, "xorg");
      setStepDone(updated, "desktop");
      setStepActive(updated, "desktop", message);
    } else if (percent <= 65) {
      setStepDone(updated, "xorg");
      setStepDone(updated, "desktop");
      setStepActive(updated, "sunshine", message);
    } else if (percent <= 80) {
      setStepDone(updated, "xorg");
      setStepDone(updated, "desktop");
      setStepDone(updated, "sunshine");
      setStepActive(updated, "display", message);
    } else if (percent <= 95) {
      setStepDone(updated, "xorg");
      setStepDone(updated, "desktop");
      setStepDone(updated, "sunshine");
      setStepDone(updated, "display");
      setStepActive(updated, "start", message);
    } else {
      updated.forEach((s) => { if (s.id !== "connect") s.status = "done"; });
      setStepActive(updated, "connect", message);
    }
  } else if (phase === "vnc_setup") {
    const gpu = updated.find((s) => s.id === "gpu");
    if (gpu) gpu.status = "done";

    if (percent <= 30) {
      setStepActive(updated, "vnc_install", message);
    } else if (percent <= 60) {
      setStepDone(updated, "vnc_install");
      setStepActive(updated, "vnc_install", message);
    } else {
      setStepDone(updated, "vnc_install");
      setStepActive(updated, "vnc_start", message);
    }
  } else if (phase === "vnc_fallback") {
    const gpu = updated.find((s) => s.id === "gpu");
    if (gpu) gpu.status = "done";
  } else if (phase === "tunnel") {
    updated.forEach((s) => {
      if (s.id !== "connect" && s.id !== "tunnel") s.status = "done";
    });
    const tunnel = updated.find((s) => s.id === "tunnel");
    if (tunnel) { tunnel.status = "active"; tunnel.detail = message; }
  } else if (phase === "proxy" || phase === "done") {
    updated.forEach((s) => {
      if (s.id !== "connect") s.status = "done";
    });
    const conn = updated.find((s) => s.id === "connect");
    if (conn) {
      conn.status = phase === "done" ? "done" : "active";
      conn.detail = message;
    }
  }

  return updated;
}

function setStepActive(steps: SetupStep[], id: string, detail?: string) {
  const step = steps.find((s) => s.id === id);
  if (step) { step.status = "active"; step.detail = detail; }
}

function setStepDone(steps: SetupStep[], id: string) {
  const step = steps.find((s) => s.id === id);
  if (step && step.status !== "done") step.status = "done";
}

// ======================== Progress Bar ========================

function ProgressBar({ percent }: { percent: number }) {
  return (
    <div className="w-full bg-surface-3 rounded-full h-2 overflow-hidden">
      <div
        className="h-full bg-gradient-to-r from-blue-500 to-accent rounded-full transition-all duration-500 ease-out"
        style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
      />
    </div>
  );
}

// ======================== Step List ========================

function StepList({ steps }: { steps: SetupStep[] }) {
  return (
    <div className="space-y-2">
      {steps.map((step) => {
        const Icon = step.icon;
        return (
          <div key={step.id} className="flex items-center gap-3">
            <div className="w-5 h-5 flex items-center justify-center shrink-0">
              {step.status === "done" && <CheckCircle2 className="w-4 h-4 text-green-400" />}
              {step.status === "active" && <Loader2 className="w-4 h-4 text-blue-400 animate-spin" />}
              {step.status === "pending" && <Circle className="w-4 h-4 text-gray-600" />}
              {step.status === "error" && <AlertCircle className="w-4 h-4 text-red-400" />}
            </div>
            <Icon
              className={`w-4 h-4 shrink-0 ${
                step.status === "done" ? "text-green-400"
                  : step.status === "active" ? "text-blue-400"
                  : step.status === "error" ? "text-red-400"
                  : "text-gray-600"
              }`}
            />
            <div className="flex-1 min-w-0">
              <span
                className={`text-sm ${
                  step.status === "done" ? "text-gray-300"
                    : step.status === "active" ? "text-white font-medium"
                    : step.status === "error" ? "text-red-400"
                    : "text-gray-500"
                }`}
              >
                {step.label}
              </span>
              {step.detail && step.status === "active" && (
                <span className="ml-2 text-xs text-gray-500">{step.detail}</span>
              )}
              {step.detail && step.status === "done" && step.id === "gpu" && (
                <span className="ml-2 text-xs text-gray-400">{step.detail}</span>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ======================== Main Component ========================

type ConnectionStatus = "setup" | "connected" | "disconnected" | "error";

export default function RemoteDesktop({
  deviceName,
  host,
  user,
  port,
  onClose,
}: RemoteDesktopProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const vncRef = useRef<VncScreenHandle>(null);
  const lastClipboardText = useRef<string>("");

  const [status, setStatus] = useState<ConnectionStatus>("setup");
  const [error, setError] = useState<string | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [mode, setMode] = useState<"sunshine" | "vnc" | null>(null);
  const [gpuName, setGpuName] = useState<string>("");
  const [streamUrl, setStreamUrl] = useState<string>("");
  const [percent, setPercent] = useState(0);
  const [steps, setSteps] = useState<SetupStep[]>([]);
  const [isFirstTime, setIsFirstTime] = useState(false);
  const [statusMessage, setStatusMessage] = useState("Initializing...");

  // ---- Fullscreen via browser Fullscreen API ----
  const toggleFullscreen = useCallback(() => {
    if (!containerRef.current) return;
    if (!document.fullscreenElement) {
      containerRef.current.requestFullscreen().then(() => setFullscreen(true)).catch(() => {});
    } else {
      document.exitFullscreen().then(() => setFullscreen(false)).catch(() => {});
    }
  }, []);

  // Sync fullscreen state when user exits via Esc
  useEffect(() => {
    const handler = () => setFullscreen(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", handler);
    return () => document.removeEventListener("fullscreenchange", handler);
  }, []);

  const connect = useCallback(async () => {
    setStatus("setup");
    setError(null);
    setPercent(0);
    setSteps([...VNC_STEPS]);
    setStatusMessage("Detecting GPU capabilities...");
    setIsFirstTime(false);

    try {
      const conn = await api.desktopConnect(host, user, port);
      setMode(conn.mode as "sunshine" | "vnc");
      setGpuName(conn.gpu_name);
      setStreamUrl(conn.url);
      setPercent(100);

      if (conn.mode === "sunshine") {
        setStatusMessage("Sunshine stream ready");
        setStatus("connected");
      } else {
        setStatusMessage("VNC connected, loading desktop...");
      }
    } catch (err) {
      setStatus("error");
      setError(String(err));
    }
  }, [host, user, port]);

  // Listen for progress events
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<{ phase: string; percent: number; message: string }>(
      "desktop-progress",
      (event) => {
        const { phase, percent: pct, message } = event.payload;
        setPercent(pct);
        setStatusMessage(message);

        if (
          phase === "sunshine_setup" &&
          (message.includes("Installing") || message.includes("Downloading"))
        ) {
          setIsFirstTime(true);
        }

        setSteps((prev) => {
          let base = prev;
          if (phase === "sunshine_setup" && prev.length !== NVENC_STEPS.length) {
            base = [...NVENC_STEPS];
          }
          return mapProgressToSteps(base, phase, pct, message);
        });
      },
    ).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  // Start connection
  useEffect(() => {
    connect();
    return () => {
      vncRef.current?.disconnect();
      api.vncDisconnect().catch(() => {});
    };
  }, [connect]);

  // Clipboard sync: local → remote on focus/mouseenter
  useEffect(() => {
    if (status !== "connected" || mode !== "vnc") return;
    const container = containerRef.current;
    if (!container) return;

    const syncClipboard = async () => {
      try {
        const text = await readText();
        if (text && text !== lastClipboardText.current) {
          lastClipboardText.current = text;
          vncRef.current?.clipboardPaste(text);
        }
      } catch {
        // clipboard may be empty or permission denied
      }
    };

    container.addEventListener("mouseenter", syncClipboard);
    window.addEventListener("focus", syncClipboard);
    return () => {
      container.removeEventListener("mouseenter", syncClipboard);
      window.removeEventListener("focus", syncClipboard);
    };
  }, [status, mode]);

  const handleClose = () => {
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {});
    }
    vncRef.current?.disconnect();
    api.vncDisconnect().catch(() => {});
    onClose();
  };

  const modeBadge =
    mode === "sunshine" ? (
      <span className="px-1.5 py-0.5 bg-green-500/20 text-green-400 rounded text-[10px] font-medium">
        NVENC H.264
      </span>
    ) : mode === "vnc" ? (
      <span className="px-1.5 py-0.5 bg-yellow-500/20 text-yellow-400 rounded text-[10px] font-medium">
        VNC
      </span>
    ) : null;

  return (
    <div ref={containerRef} className="flex flex-col bg-surface-0 h-full">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-surface-1 border-b border-surface-3 shrink-0">
        <div className="flex items-center gap-2">
          <span className="text-sm text-white font-medium">{deviceName}</span>
          <span className="text-xs text-gray-500">{host}</span>
          {modeBadge}
          {gpuName && (
            <span className="flex items-center gap-1 text-[10px] text-gray-500">
              <Cpu className="w-3 h-3" />
              {gpuName}
            </span>
          )}
          {status === "setup" && (
            <Loader2 className="w-3.5 h-3.5 text-blue-400 animate-spin" />
          )}
          {status === "connected" && (
            <span className="w-2 h-2 rounded-full bg-green-400" />
          )}
        </div>
        <div className="flex items-center gap-1">
          {status === "connected" && (
            <button
              onClick={toggleFullscreen}
              className="p-1 text-gray-400 hover:text-white transition-colors"
              title={fullscreen ? "Exit fullscreen (Esc)" : "Enter fullscreen"}
            >
              {fullscreen ? <Minimize2 className="w-4 h-4" /> : <Maximize2 className="w-4 h-4" />}
            </button>
          )}
          <button
            onClick={handleClose}
            className="p-1 text-gray-400 hover:text-red-400 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Content area */}
      <div className="flex-1 overflow-hidden bg-black relative">
        {/* Sunshine mode: iframe */}
        {mode === "sunshine" && streamUrl && status === "connected" && (
          <iframe
            ref={iframeRef}
            src={streamUrl}
            className="w-full h-full border-none"
            allow="autoplay; clipboard-read; clipboard-write; fullscreen; pointer-lock; keyboard-map"
          />
        )}

        {/* VNC mode: react-vnc with quality optimization */}
        {mode === "vnc" && streamUrl && (
          <VncScreen
            ref={vncRef}
            url={streamUrl}
            scaleViewport
            showDotCursor
            qualityLevel={8}
            compressionLevel={2}
            style={{
              width: "100%",
              height: "100%",
              display: status === "connected" ? "block" : "none",
            }}
            onConnect={() => {
              setStatus("connected");
              setStatusMessage("Connected");
            }}
            onClipboard={(e: { detail: { text: string } }) => {
              const text = e?.detail?.text;
              if (text) {
                lastClipboardText.current = text;
                writeText(text).catch(() => {});
              }
            }}
            onDisconnect={(e: { detail: { clean: boolean } }) => {
              if (e?.detail?.clean) {
                setStatus("disconnected");
              } else {
                setStatus("error");
                setError("VNC connection lost");
              }
            }}
          />
        )}

        {/* Setup overlay */}
        {status === "setup" && (
          <div className="absolute inset-0 flex items-center justify-center bg-surface-0">
            <div className="w-full max-w-md px-8">
              <div className="text-center mb-6">
                <Monitor className="w-12 h-12 text-accent mx-auto mb-3" />
                <h2 className="text-lg font-semibold text-white mb-1">
                  Setting up Remote Desktop
                </h2>
                <p className="text-sm text-gray-400">{host}</p>
              </div>
              <div className="mb-2">
                <ProgressBar percent={percent} />
              </div>
              <div className="flex justify-between text-xs text-gray-500 mb-6">
                <span>{statusMessage}</span>
                <span>{percent}%</span>
              </div>
              <StepList steps={steps} />
              {isFirstTime && (
                <div className="mt-6 p-3 bg-blue-500/10 border border-blue-500/20 rounded-lg">
                  <p className="text-xs text-blue-300">
                    <strong>First-time setup detected.</strong> Installing desktop
                    environment and streaming server. This may take 3-5 minutes
                    depending on network speed. Subsequent connections will be much
                    faster.
                  </p>
                </div>
              )}
            </div>
          </div>
        )}

        {/* Error overlay */}
        {status === "error" && (
          <div className="absolute inset-0 flex items-center justify-center bg-surface-0">
            <div className="text-center max-w-md px-8">
              <AlertCircle className="w-10 h-10 text-red-400 mx-auto mb-3" />
              <p className="text-red-400 text-sm font-medium mb-2">Connection Failed</p>
              <p className="text-gray-400 text-xs mb-4 break-words">{error}</p>
              <div className="flex gap-2 justify-center">
                <button
                  onClick={connect}
                  className="flex items-center gap-1.5 px-4 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-sm"
                >
                  <RefreshCw className="w-3.5 h-3.5" />
                  Retry
                </button>
                <button
                  onClick={handleClose}
                  className="px-4 py-1.5 bg-surface-3 hover:bg-surface-2 text-gray-300 rounded text-sm"
                >
                  Close
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Disconnected overlay */}
        {status === "disconnected" && (
          <div className="absolute inset-0 flex items-center justify-center bg-black/80">
            <div className="text-center">
              <p className="text-gray-300 text-sm mb-3">Disconnected</p>
              <div className="flex gap-2 justify-center">
                <button
                  onClick={connect}
                  className="flex items-center gap-1.5 px-4 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-sm"
                >
                  <RefreshCw className="w-3.5 h-3.5" />
                  Reconnect
                </button>
                <button
                  onClick={handleClose}
                  className="px-4 py-1.5 bg-surface-3 hover:bg-surface-2 text-gray-300 rounded text-sm"
                >
                  Close
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

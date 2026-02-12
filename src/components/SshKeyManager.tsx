import { useState, useEffect, useCallback } from "react";
import {
  Key,
  Plus,
  Copy,
  Upload,
  RefreshCw,
  Loader2,
  AlertCircle,
  Check,
  ChevronDown,
  X,
} from "lucide-react";
import * as api from "../services/api";
import type { SshKeyInfo } from "../services/api";
import type { Device } from "../services/types";

export default function SshKeyManager() {
  const [keys, setKeys] = useState<SshKeyInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);

  // Generate form state
  const [showGenerate, setShowGenerate] = useState(false);
  const [genName, setGenName] = useState("");
  const [genPassphrase, setGenPassphrase] = useState("");
  const [generating, setGenerating] = useState(false);

  // Deploy state
  const [deployKeyPath, setDeployKeyPath] = useState<string | null>(null);
  const [devices, setDevices] = useState<(Device & { online: boolean })[]>([]);
  const [deploying, setDeploying] = useState(false);
  const [deployDevice, setDeployDevice] = useState<string | null>(null);
  const [showDevicePicker, setShowDevicePicker] = useState(false);

  // Clipboard feedback per key
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  const loadKeys = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await api.sshKeysList();
      setKeys(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadKeys();
  }, [loadKeys]);

  const handleGenerate = async () => {
    if (!genName.trim()) return;
    setGenerating(true);
    setError(null);
    try {
      await api.sshKeyGenerate(genName.trim(), genPassphrase);
      setShowGenerate(false);
      setGenName("");
      setGenPassphrase("");
      setSuccessMsg(`Key "${genName.trim()}" generated successfully`);
      setTimeout(() => setSuccessMsg(null), 3000);
      await loadKeys();
    } catch (err) {
      setError(String(err));
    } finally {
      setGenerating(false);
    }
  };

  const handleCopy = async (key: SshKeyInfo) => {
    if (!key.public_key) return;
    try {
      await navigator.clipboard.writeText(key.public_key);
      setCopiedKey(key.name);
      setTimeout(() => setCopiedKey(null), 2000);
    } catch {
      setError("Failed to copy to clipboard");
    }
  };

  const handleDeployOpen = async (keyPath: string) => {
    setDeployKeyPath(keyPath);
    setShowDevicePicker(true);
    setDeployDevice(null);
    try {
      const devs = await api.listDevices();
      setDevices(devs);
    } catch {
      setDevices([]);
    }
  };

  const handleDeploy = async () => {
    if (!deployKeyPath || !deployDevice) return;
    const device = devices.find((d) => d.id === deployDevice);
    if (!device) return;

    setDeploying(true);
    setError(null);
    try {
      const host = device.ssh_host || device.vpn_ip;
      await api.sshKeyCopyToRemote(
        deployKeyPath,
        host,
        device.ssh_user,
        device.ssh_port,
      );
      setShowDevicePicker(false);
      setDeployKeyPath(null);
      setDeployDevice(null);
      setSuccessMsg(`Key deployed to ${device.name}`);
      setTimeout(() => setSuccessMsg(null), 3000);
    } catch (err) {
      setError(String(err));
    } finally {
      setDeploying(false);
    }
  };

  const formatFingerprint = (fp: string | null): string => {
    if (!fp) return "N/A";
    // Fingerprint format: "256 SHA256:xxxx comment (TYPE)"
    // Extract just the SHA256 hash portion for brevity
    const match = fp.match(/(SHA256:\S+)/);
    return match ? match[1] : fp;
  };

  const keyTypeBadgeColor = (type_: string): string => {
    switch (type_) {
      case "ed25519":
        return "bg-green-500/20 text-green-400";
      case "rsa":
        return "bg-blue-500/20 text-blue-400";
      case "ecdsa":
        return "bg-purple-500/20 text-purple-400";
      default:
        return "bg-gray-500/20 text-gray-400";
    }
  };

  return (
    <div className="flex flex-col h-full bg-surface-0">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 bg-surface-1 border-b border-surface-3 shrink-0">
        <div className="flex items-center gap-2">
          <Key className="w-4 h-4 text-accent" />
          <h2 className="text-sm font-medium text-white">SSH Keys</h2>
          <span className="text-xs text-gray-500">
            {keys.length} key{keys.length !== 1 ? "s" : ""}
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => setShowGenerate(!showGenerate)}
            className="flex items-center gap-1 px-2.5 py-1 bg-accent/20 hover:bg-accent/30 text-accent rounded text-xs transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            Generate New Key
          </button>
          <button
            onClick={loadKeys}
            disabled={loading}
            className="p-1.5 text-gray-400 hover:text-white transition-colors rounded hover:bg-surface-3"
            title="Refresh"
          >
            <RefreshCw
              className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`}
            />
          </button>
        </div>
      </div>

      {/* Success banner */}
      {successMsg && (
        <div className="flex items-center gap-2 px-4 py-2 bg-green-500/10 border-b border-green-500/20 shrink-0">
          <Check className="w-4 h-4 text-green-400 shrink-0" />
          <span className="text-xs text-green-400 flex-1">{successMsg}</span>
        </div>
      )}

      {/* Error banner */}
      {error && (
        <div className="flex items-center gap-2 px-4 py-2 bg-red-500/10 border-b border-red-500/20 shrink-0">
          <AlertCircle className="w-4 h-4 text-red-400 shrink-0" />
          <span className="text-xs text-red-400 flex-1">{error}</span>
          <button
            onClick={() => setError(null)}
            className="text-red-400 hover:text-red-300 text-xs"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Generate form */}
      {showGenerate && (
        <div className="mx-4 mt-3 p-4 bg-surface-2 border border-surface-3 rounded-lg shrink-0">
          <h3 className="text-sm font-medium text-white mb-3">
            Generate New SSH Key
          </h3>
          <div className="flex flex-col gap-2 mb-3">
            <input
              autoFocus
              type="text"
              placeholder="Key name (e.g. my-server)"
              value={genName}
              onChange={(e) => setGenName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleGenerate();
                if (e.key === "Escape") {
                  setShowGenerate(false);
                  setGenName("");
                  setGenPassphrase("");
                }
              }}
              className="px-3 py-1.5 bg-surface-0 border border-surface-3 rounded text-sm text-white placeholder-gray-500 focus:outline-none focus:border-accent"
            />
            <input
              type="password"
              placeholder="Passphrase (optional, leave empty for none)"
              value={genPassphrase}
              onChange={(e) => setGenPassphrase(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleGenerate();
              }}
              className="px-3 py-1.5 bg-surface-0 border border-surface-3 rounded text-sm text-white placeholder-gray-500 focus:outline-none focus:border-accent"
            />
          </div>
          <p className="text-[11px] text-gray-500 mb-3">
            Generates an Ed25519 key in ~/.ssh/{genName || "<name>"}
          </p>
          <div className="flex justify-end gap-2">
            <button
              onClick={() => {
                setShowGenerate(false);
                setGenName("");
                setGenPassphrase("");
              }}
              className="px-3 py-1.5 text-sm text-gray-400 hover:text-white transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleGenerate}
              disabled={generating || !genName.trim()}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-sm transition-colors disabled:opacity-50"
            >
              {generating && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
              Generate
            </button>
          </div>
        </div>
      )}

      {/* Device picker modal for deploy */}
      {showDevicePicker && (
        <div className="mx-4 mt-3 p-4 bg-surface-2 border border-surface-3 rounded-lg shrink-0">
          <div className="flex items-center justify-between mb-3">
            <h3 className="text-sm font-medium text-white">
              Deploy Key to Server
            </h3>
            <button
              onClick={() => {
                setShowDevicePicker(false);
                setDeployKeyPath(null);
                setDeployDevice(null);
              }}
              className="p-1 text-gray-400 hover:text-white"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
          <div className="relative mb-3">
            <button
              onClick={() => {
                // Toggle a simple dropdown
                const el = document.getElementById("device-dropdown");
                if (el) el.classList.toggle("hidden");
              }}
              className="w-full flex items-center justify-between px-3 py-1.5 bg-surface-0 border border-surface-3 rounded text-sm text-white focus:outline-none focus:border-accent"
            >
              <span className={deployDevice ? "text-white" : "text-gray-500"}>
                {deployDevice
                  ? devices.find((d) => d.id === deployDevice)?.name ??
                    "Select device"
                  : "Select a device..."}
              </span>
              <ChevronDown className="w-3.5 h-3.5 text-gray-400" />
            </button>
            <div
              id="device-dropdown"
              className="hidden absolute z-10 w-full mt-1 bg-surface-1 border border-surface-3 rounded shadow-lg max-h-48 overflow-auto"
            >
              {devices.length === 0 ? (
                <div className="px-3 py-2 text-xs text-gray-500">
                  No devices configured
                </div>
              ) : (
                devices.map((device) => (
                  <button
                    key={device.id}
                    onClick={() => {
                      setDeployDevice(device.id);
                      const el = document.getElementById("device-dropdown");
                      if (el) el.classList.add("hidden");
                    }}
                    className={`w-full text-left px-3 py-2 text-sm hover:bg-surface-3 transition-colors flex items-center justify-between ${
                      deployDevice === device.id
                        ? "text-accent"
                        : "text-gray-300"
                    }`}
                  >
                    <span>{device.name}</span>
                    <span className="text-[10px] text-gray-500">
                      {device.ssh_host || device.vpn_ip}
                      {device.online ? (
                        <span className="ml-1.5 text-green-400">online</span>
                      ) : (
                        <span className="ml-1.5 text-gray-600">offline</span>
                      )}
                    </span>
                  </button>
                ))
              )}
            </div>
          </div>
          <div className="flex justify-end gap-2">
            <button
              onClick={() => {
                setShowDevicePicker(false);
                setDeployKeyPath(null);
                setDeployDevice(null);
              }}
              className="px-3 py-1.5 text-sm text-gray-400 hover:text-white transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleDeploy}
              disabled={deploying || !deployDevice}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-sm transition-colors disabled:opacity-50"
            >
              {deploying && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
              Deploy
            </button>
          </div>
        </div>
      )}

      {/* Key list */}
      <div className="flex-1 overflow-auto px-4 py-3">
        {loading && keys.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="w-6 h-6 text-accent animate-spin" />
          </div>
        ) : keys.length === 0 ? (
          <div className="text-center text-gray-500 py-12">
            <Key className="w-8 h-8 mx-auto mb-3 text-gray-600" />
            <p className="text-sm">No SSH keys found in ~/.ssh/</p>
            <p className="text-xs mt-1">
              Click "Generate New Key" to create one.
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {keys.map((key) => (
              <div
                key={key.name}
                className="p-3 bg-surface-2 border border-surface-3 rounded-lg hover:border-surface-1 transition-colors"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <Key className="w-3.5 h-3.5 text-gray-400 shrink-0" />
                      <span className="text-sm font-medium text-white truncate">
                        {key.name}
                      </span>
                      <span
                        className={`px-1.5 py-0.5 rounded text-[10px] font-medium uppercase ${keyTypeBadgeColor(key.key_type)}`}
                      >
                        {key.key_type}
                      </span>
                    </div>
                    <div className="text-[11px] text-gray-500 font-mono truncate mb-1">
                      {formatFingerprint(key.fingerprint)}
                    </div>
                    <div className="text-[10px] text-gray-600 truncate">
                      {key.path}
                    </div>
                  </div>
                  <div className="flex items-center gap-1 shrink-0">
                    {key.has_public && key.public_key && (
                      <button
                        onClick={() => handleCopy(key)}
                        className="flex items-center gap-1 px-2 py-1 bg-surface-3 hover:bg-surface-1 text-gray-300 hover:text-white rounded text-xs transition-colors"
                        title="Copy public key to clipboard"
                      >
                        {copiedKey === key.name ? (
                          <>
                            <Check className="w-3 h-3 text-green-400" />
                            <span className="text-green-400">Copied</span>
                          </>
                        ) : (
                          <>
                            <Copy className="w-3 h-3" />
                            <span>Copy Public Key</span>
                          </>
                        )}
                      </button>
                    )}
                    {key.has_public && (
                      <button
                        onClick={() => handleDeployOpen(key.path)}
                        className="flex items-center gap-1 px-2 py-1 bg-surface-3 hover:bg-surface-1 text-gray-300 hover:text-white rounded text-xs transition-colors"
                        title="Deploy public key to a remote server"
                      >
                        <Upload className="w-3 h-3" />
                        <span>Deploy to Server</span>
                      </button>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

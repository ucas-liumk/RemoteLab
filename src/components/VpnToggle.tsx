import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Shield, ShieldOff, Loader2, Wifi } from "lucide-react";
import type { VpnStatus } from "../services/types";
import * as api from "../services/api";

export default function VpnToggle() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<VpnStatus>({ connected: false });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hasConfig, setHasConfig] = useState<boolean | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await api.vpnStatus();
      setStatus(s);
      setError(null);
    } catch {
      // VPN tools not available, keep current state
    }
  }, []);

  useEffect(() => {
    api.vpnHasConfig().then(setHasConfig).catch(() => setHasConfig(false));
  }, []);

  useEffect(() => {
    if (hasConfig === false) return;
    refreshStatus();
    const interval = setInterval(refreshStatus, 5000);
    return () => clearInterval(interval);
  }, [refreshStatus, hasConfig]);

  if (hasConfig === null || hasConfig === false) {
    return null;
  }

  const toggle = async () => {
    setLoading(true);
    setError(null);
    try {
      if (status.connected) {
        await api.vpnDisconnect();
      } else {
        // Empty string uses the stored WireGuard config path from settings
        await api.vpnConnect("");
      }
      // Wait for status refresh
      setTimeout(refreshStatus, 500);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex items-center gap-3 px-4 py-2 bg-surface-1 border-b border-surface-3">
      <button
        onClick={toggle}
        disabled={loading}
        className={`flex items-center gap-2 px-4 py-1.5 rounded-full text-sm font-medium transition-all ${
          status.connected
            ? "bg-vpn-on/20 text-vpn-on border border-vpn-on/30 hover:bg-vpn-on/30"
            : "bg-surface-3 text-gray-500 dark:text-gray-400 border border-surface-3 hover:text-gray-900 dark:hover:text-white hover:border-gray-500"
        }`}
      >
        {loading ? (
          <Loader2 className="w-4 h-4 animate-spin" />
        ) : status.connected ? (
          <Shield className="w-4 h-4" />
        ) : (
          <ShieldOff className="w-4 h-4" />
        )}
        <span>
          {loading
            ? t('vpn.connecting')
            : status.connected
              ? t('vpn.connected')
              : t('vpn.off')}
        </span>
      </button>

      {status.connected && (
        <div className="flex items-center gap-4 text-xs text-gray-500 dark:text-gray-400">
          <span className="flex items-center gap-1">
            <Wifi className="w-3 h-3" />
            {status.local_ip || "..."}
          </span>
          {status.latency_ms != null && (
            <span
              className={`${
                status.latency_ms < 100
                  ? "text-vpn-on"
                  : status.latency_ms < 300
                    ? "text-yellow-400"
                    : "text-red-400"
              }`}
            >
              {status.latency_ms}ms
            </span>
          )}
        </div>
      )}

      {error && (
        <span className="text-xs text-red-400 truncate max-w-[300px]">
          {error}
        </span>
      )}
    </div>
  );
}

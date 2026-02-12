import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Plus, RefreshCw, Settings as SettingsIcon } from "lucide-react";
import VpnToggle from "./VpnToggle";
import DeviceCard from "./DeviceCard";
import type { Device } from "../services/types";
import * as api from "../services/api";

interface DashboardProps {
  onOpenTerminal: (name: string, host: string, user: string, port?: number) => void;
  onOpenDesktop: (name: string, host: string, user: string, port?: number) => void;
  onOpenFiles: (name: string, host: string, user: string, port?: number) => void;
  onNavigate: (view: string) => void;
}

type DeviceWithStatus = Device & { online: boolean };

export default function Dashboard({ onOpenTerminal, onOpenDesktop, onOpenFiles, onNavigate }: DashboardProps) {
  const { t } = useTranslation();
  const [devices, setDevices] = useState<DeviceWithStatus[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [newDevice, setNewDevice] = useState({
    name: "",
    vpn_ip: "",
    ssh_user: "root",
    rustdesk_id: "",
    ssh_host: "",
    ssh_port: "",
  });

  const loadDevices = useCallback(async () => {
    try {
      const list = await api.listDevices();
      setDevices(list);
    } catch {
      setDevices([]);
    }
  }, []);

  useEffect(() => {
    loadDevices();
  }, [loadDevices]);

  const refresh = async () => {
    setRefreshing(true);
    await loadDevices();
    setRefreshing(false);
  };

  const handleAddDevice = async () => {
    if (!newDevice.name || (!newDevice.vpn_ip && !newDevice.ssh_host)) return;
    try {
      await api.addDevice(
        newDevice.name,
        newDevice.vpn_ip || newDevice.ssh_host,
        newDevice.ssh_user,
        newDevice.rustdesk_id || undefined,
        newDevice.ssh_host || undefined,
        newDevice.ssh_port ? parseInt(newDevice.ssh_port) : undefined,
      );
      setShowAddForm(false);
      setNewDevice({ name: "", vpn_ip: "", ssh_user: "root", rustdesk_id: "", ssh_host: "", ssh_port: "" });
      await loadDevices();
    } catch (err) {
      console.error("Failed to add device:", err);
    }
  };

  const handleRemoveDevice = async (id: string) => {
    try {
      await api.removeDevice(id);
      await loadDevices();
    } catch (err) {
      console.error("Failed to remove device:", err);
    }
  };

  return (
    <div className="flex flex-col h-full">
      {/* VPN status bar */}
      <VpnToggle />

      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3">
        <h1 className="text-lg font-semibold text-gray-900 dark:text-white">{t('dashboard.devices')}</h1>
        <div className="flex items-center gap-2">
          <button
            onClick={refresh}
            className="p-1.5 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors rounded hover:bg-surface-3"
            title="Refresh"
          >
            <RefreshCw
              className={`w-4 h-4 ${refreshing ? "animate-spin" : ""}`}
            />
          </button>
          <button
            onClick={() => setShowAddForm(true)}
            className="flex items-center gap-1 px-3 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-sm transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            {t('dashboard.addDevice')}
          </button>
          <button
            onClick={() => onNavigate("settings")}
            className="p-1.5 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors rounded hover:bg-surface-3"
            title="Settings"
          >
            <SettingsIcon className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Add device form */}
      {showAddForm && (
        <div className="mx-4 mb-3 p-4 bg-surface-2 border border-surface-3 rounded-lg">
          <h3 className="text-sm font-medium text-gray-900 dark:text-white mb-3">{t('dashboard.addNewDevice')}</h3>
          <div className="grid grid-cols-2 gap-2 mb-3">
            <input
              type="text"
              placeholder={t('dashboard.deviceName')}
              value={newDevice.name}
              onChange={(e) =>
                setNewDevice({ ...newDevice, name: e.target.value })
              }
              className="px-3 py-1.5 bg-surface-0 border border-surface-3 rounded text-sm text-gray-900 dark:text-white placeholder-gray-400 dark:placeholder-gray-500 focus:outline-none focus:border-accent"
            />
            <input
              type="text"
              placeholder={t('dashboard.sshHost')}
              value={newDevice.ssh_host}
              onChange={(e) =>
                setNewDevice({ ...newDevice, ssh_host: e.target.value })
              }
              className="px-3 py-1.5 bg-surface-0 border border-surface-3 rounded text-sm text-gray-900 dark:text-white placeholder-gray-400 dark:placeholder-gray-500 focus:outline-none focus:border-accent"
            />
            <input
              type="text"
              placeholder={t('dashboard.sshUser')}
              value={newDevice.ssh_user}
              onChange={(e) =>
                setNewDevice({ ...newDevice, ssh_user: e.target.value })
              }
              className="px-3 py-1.5 bg-surface-0 border border-surface-3 rounded text-sm text-gray-900 dark:text-white placeholder-gray-400 dark:placeholder-gray-500 focus:outline-none focus:border-accent"
            />
            <input
              type="text"
              placeholder={t('dashboard.sshPort')}
              value={newDevice.ssh_port}
              onChange={(e) =>
                setNewDevice({ ...newDevice, ssh_port: e.target.value.replace(/\D/g, "") })
              }
              className="px-3 py-1.5 bg-surface-0 border border-surface-3 rounded text-sm text-gray-900 dark:text-white placeholder-gray-400 dark:placeholder-gray-500 focus:outline-none focus:border-accent"
            />
          </div>
          <div className="flex justify-end gap-2">
            <button
              onClick={() => setShowAddForm(false)}
              className="px-3 py-1.5 text-sm text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
            >
              {t('dashboard.cancel')}
            </button>
            <button
              onClick={handleAddDevice}
              className="px-3 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-sm transition-colors"
            >
              {t('dashboard.add')}
            </button>
          </div>
        </div>
      )}

      {/* Device list */}
      <div className="flex-1 overflow-auto px-4 pb-4">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
          {devices.map((device) => (
            <DeviceCard
              key={device.id}
              device={device}
              onOpenTerminal={onOpenTerminal}
              onOpenDesktop={onOpenDesktop}
              onOpenFiles={onOpenFiles}
              onRemove={() => handleRemoveDevice(device.id)}
            />
          ))}
        </div>

        {devices.length === 0 && (
          <div className="text-center text-gray-400 dark:text-gray-500 py-12">
            <p>{t('dashboard.noDevices')}</p>
            <p className="text-sm mt-1">
              {t('dashboard.noDevicesHint')}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

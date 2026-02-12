import { useTranslation } from "react-i18next";
import {
  Monitor,
  Terminal as TerminalIcon,
  Trash2,
  FolderOpen,
} from "lucide-react";
import type { Device } from "../services/types";

interface DeviceCardProps {
  device: Device & { online: boolean };
  onOpenTerminal: (name: string, host: string, user: string, port?: number) => void;
  onOpenDesktop: (name: string, host: string, user: string, port?: number) => void;
  onOpenFiles: (name: string, host: string, user: string, port?: number) => void;
  onRemove: () => void;
}

export default function DeviceCard({
  device,
  onOpenTerminal,
  onOpenDesktop,
  onOpenFiles,
  onRemove,
}: DeviceCardProps) {
  const { t } = useTranslation();
  const sshHost = device.ssh_host || device.vpn_ip;
  const sshPort = device.ssh_port;

  return (
    <div className="bg-surface-2 rounded-lg p-4 border border-surface-3 hover:border-accent/30 transition-colors">
      <div className="flex items-start justify-between mb-3">
        <div className="flex items-center gap-2">
          <div
            className={`w-2 h-2 rounded-full ${
              device.online ? "bg-vpn-on animate-pulse" : "bg-gray-600"
            }`}
          />
          <h3 className="font-medium text-gray-900 dark:text-white">{device.name}</h3>
        </div>
        <button
          onClick={onRemove}
          className="text-gray-400 dark:text-gray-600 hover:text-red-400 transition-colors p-1"
          title={t('device.remove')}
        >
          <Trash2 className="w-3.5 h-3.5" />
        </button>
      </div>

      <div className="text-xs text-gray-500 dark:text-gray-400 mb-3 space-y-0.5">
        <div>
          {t('device.host')}: <span className="text-gray-600 dark:text-gray-300">{sshHost}{sshPort ? `:${sshPort}` : ""}</span>
        </div>
        <div>
          {t('device.user')}: <span className="text-gray-600 dark:text-gray-300">{device.ssh_user}</span>
        </div>
      </div>

      <div className="flex gap-1.5">
        <button
          onClick={() =>
            onOpenTerminal(device.name, sshHost, device.ssh_user, sshPort)
          }
          className="flex items-center gap-1 px-2.5 py-1.5 bg-surface-3 hover:bg-accent/20 hover:text-accent-hover text-gray-600 dark:text-gray-300 rounded text-xs transition-colors"
          title="SSH Terminal"
        >
          <TerminalIcon className="w-3.5 h-3.5" />
          {t('device.ssh')}
        </button>
        <button
          onClick={() =>
            onOpenDesktop(device.name, sshHost, device.ssh_user, sshPort)
          }
          className="flex items-center gap-1 px-2.5 py-1.5 bg-surface-3 hover:bg-accent/20 hover:text-accent-hover text-gray-600 dark:text-gray-300 rounded text-xs transition-colors"
          title="Remote Desktop"
        >
          <Monitor className="w-3.5 h-3.5" />
          {t('device.desktop')}
        </button>
        <button
          onClick={() =>
            onOpenFiles(device.name, sshHost, device.ssh_user, sshPort)
          }
          className="flex items-center gap-1 px-2.5 py-1.5 bg-surface-3 hover:bg-accent/20 hover:text-accent-hover text-gray-600 dark:text-gray-300 rounded text-xs transition-colors"
          title="File Manager"
        >
          <FolderOpen className="w-3.5 h-3.5" />
          {t('device.files')}
        </button>
      </div>
    </div>
  );
}

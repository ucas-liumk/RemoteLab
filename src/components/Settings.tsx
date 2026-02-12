import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  FolderOpen,
  Download,
  Upload,
  Sun,
  Moon,
  Globe,
  Key,
  Lock,
  LockOpen,
  Loader2,
} from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { useTheme } from "../contexts/ThemeContext";
import SshKeyManager from "./SshKeyManager";
import * as api from "../services/api";

interface SettingsProps {
  onNavigate: (view: "dashboard" | "settings") => void;
}

export default function Settings({ onNavigate }: SettingsProps) {
  const { t, i18n } = useTranslation();
  const { theme, toggleTheme } = useTheme();
  const [wgConfigPath, setWgConfigPath] = useState("");
  const [importStatus, setImportStatus] = useState<string | null>(null);
  const [configStatus, setConfigStatus] = useState<string | null>(null);
  const [showSshKeys, setShowSshKeys] = useState(false);

  // Encryption state
  const [isEncrypted, setIsEncrypted] = useState(false);
  const [encPassword, setEncPassword] = useState("");
  const [encLoading, setEncLoading] = useState(false);
  const [encStatus, setEncStatus] = useState<string | null>(null);

  useEffect(() => {
    api.configIsEncrypted().then(setIsEncrypted).catch(() => {});
  }, []);

  const handleExportConfig = async () => {
    try {
      setConfigStatus(null);
      const json = await api.exportConfig();
      const filePath = await save({
        defaultPath: "remotelab-config.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (filePath) {
        await writeTextFile(filePath, json);
        setConfigStatus(t("settings.exportSuccess"));
      }
    } catch (err) {
      setConfigStatus(`Export error: ${err}`);
    }
  };

  const handleImportDeviceConfig = async () => {
    try {
      setConfigStatus(null);
      const selected = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (selected) {
        const json = await readTextFile(selected);
        await api.importConfig(json);
        setConfigStatus(t("settings.importSuccess"));
      }
    } catch (err) {
      setConfigStatus(`Import error: ${err}`);
    }
  };

  const handleImportWgConfig = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "WireGuard Config", extensions: ["conf"] }],
      });
      if (selected) {
        const dest = await api.vpnImportConfig(selected);
        setWgConfigPath(dest);
        setImportStatus("Config imported successfully");
      }
    } catch (err) {
      setImportStatus(`Error: ${err}`);
    }
  };

  const handleSetPassword = async () => {
    if (!encPassword) return;
    setEncLoading(true);
    setEncStatus(null);
    try {
      await api.setConfigPassword(encPassword);
      setIsEncrypted(true);
      setEncPassword("");
      setEncStatus("Encryption enabled");
    } catch (err) {
      setEncStatus(`Error: ${err}`);
    } finally {
      setEncLoading(false);
    }
  };

  const handleRemovePassword = async () => {
    setEncLoading(true);
    setEncStatus(null);
    try {
      await api.removeConfigPassword();
      setIsEncrypted(false);
      setEncStatus("Encryption removed");
    } catch (err) {
      setEncStatus(`Error: ${err}`);
    } finally {
      setEncLoading(false);
    }
  };

  const changeLanguage = (lang: string) => {
    i18n.changeLanguage(lang);
  };

  if (showSshKeys) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center gap-3 px-4 py-3 border-b border-surface-3">
          <button
            onClick={() => setShowSshKeys(false)}
            className="p-1 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
          >
            <ArrowLeft className="w-5 h-5" />
          </button>
          <h1 className="text-lg font-semibold text-gray-900 dark:text-white">
            {t("settings.sshKeys")}
          </h1>
        </div>
        <div className="flex-1 overflow-hidden">
          <SshKeyManager />
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 px-4 py-3 border-b border-surface-3">
        <button
          onClick={() => onNavigate("dashboard")}
          className="p-1 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
        >
          <ArrowLeft className="w-5 h-5" />
        </button>
        <h1 className="text-lg font-semibold text-gray-900 dark:text-white">
          {t("settings.title")}
        </h1>
      </div>

      <div className="flex-1 overflow-auto p-4 space-y-6">
        {/* Theme & Language */}
        <section>
          <h2 className="text-sm font-medium text-gray-600 dark:text-gray-300 mb-3">
            {t("settings.theme")} & {t("settings.language")}
          </h2>
          <div className="bg-surface-2 rounded-lg p-4 border border-surface-3 space-y-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-300">
                {theme === "dark" ? (
                  <Moon className="w-4 h-4" />
                ) : (
                  <Sun className="w-4 h-4" />
                )}
                {t("settings.theme")}
              </div>
              <button
                onClick={toggleTheme}
                className="px-3 py-1 bg-surface-3 hover:bg-accent/20 text-gray-600 dark:text-gray-300 rounded text-sm transition-colors"
              >
                {theme === "dark" ? t("settings.light") : t("settings.dark")}
              </button>
            </div>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-300">
                <Globe className="w-4 h-4" />
                {t("settings.language")}
              </div>
              <div className="flex gap-1">
                <button
                  onClick={() => changeLanguage("en")}
                  className={`px-3 py-1 rounded text-sm transition-colors ${
                    i18n.language?.startsWith("en")
                      ? "bg-accent text-white"
                      : "bg-surface-3 text-gray-600 dark:text-gray-300 hover:bg-accent/20"
                  }`}
                >
                  English
                </button>
                <button
                  onClick={() => changeLanguage("zh")}
                  className={`px-3 py-1 rounded text-sm transition-colors ${
                    i18n.language?.startsWith("zh")
                      ? "bg-accent text-white"
                      : "bg-surface-3 text-gray-600 dark:text-gray-300 hover:bg-accent/20"
                  }`}
                >
                  中文
                </button>
              </div>
            </div>
          </div>
        </section>

        {/* WireGuard Config */}
        <section>
          <h2 className="text-sm font-medium text-gray-600 dark:text-gray-300 mb-3">
            {t("settings.wireguard")}
          </h2>
          <div className="bg-surface-2 rounded-lg p-4 border border-surface-3 space-y-3">
            <div>
              <label className="block text-xs text-gray-500 dark:text-gray-400 mb-1">
                {t("settings.configPath")}
              </label>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={wgConfigPath}
                  readOnly
                  placeholder="No config imported"
                  className="flex-1 px-3 py-1.5 bg-surface-0 border border-surface-3 rounded text-sm text-gray-900 dark:text-white placeholder-gray-400 dark:placeholder-gray-600"
                />
                <button
                  onClick={handleImportWgConfig}
                  className="flex items-center gap-1 px-3 py-1.5 bg-surface-3 hover:bg-accent/20 text-gray-600 dark:text-gray-300 rounded text-sm transition-colors"
                >
                  <FolderOpen className="w-4 h-4" />
                  {t("settings.import")}
                </button>
              </div>
              {importStatus && (
                <p
                  className={`text-xs mt-1 ${importStatus.startsWith("Error") ? "text-red-400" : "text-vpn-on"}`}
                >
                  {importStatus}
                </p>
              )}
            </div>
          </div>
        </section>

        {/* Device Configuration */}
        <section>
          <h2 className="text-sm font-medium text-gray-600 dark:text-gray-300 mb-3">
            {t("settings.deviceConfig")}
          </h2>
          <div className="bg-surface-2 rounded-lg p-4 border border-surface-3 space-y-3">
            <p className="text-xs text-gray-500 dark:text-gray-400">
              Export your device list and settings to a JSON file, or import a
              previously exported configuration.
            </p>
            <div className="flex gap-2">
              <button
                onClick={handleExportConfig}
                className="flex items-center gap-1 px-3 py-1.5 bg-surface-3 hover:bg-accent/20 text-gray-600 dark:text-gray-300 rounded text-sm transition-colors"
              >
                <Download className="w-4 h-4" />
                {t("settings.exportConfig")}
              </button>
              <button
                onClick={handleImportDeviceConfig}
                className="flex items-center gap-1 px-3 py-1.5 bg-surface-3 hover:bg-accent/20 text-gray-600 dark:text-gray-300 rounded text-sm transition-colors"
              >
                <Upload className="w-4 h-4" />
                {t("settings.importConfig")}
              </button>
            </div>
            {configStatus && (
              <p
                className={`text-xs mt-1 ${configStatus.includes("error") || configStatus.includes("Error") ? "text-red-400" : "text-vpn-on"}`}
              >
                {configStatus}
              </p>
            )}
          </div>
        </section>

        {/* SSH Keys */}
        <section>
          <h2 className="text-sm font-medium text-gray-600 dark:text-gray-300 mb-3">
            {t("settings.sshKeys")}
          </h2>
          <div className="bg-surface-2 rounded-lg p-4 border border-surface-3">
            <button
              onClick={() => setShowSshKeys(true)}
              className="flex items-center gap-2 px-3 py-2 bg-surface-3 hover:bg-accent/20 text-gray-600 dark:text-gray-300 rounded text-sm transition-colors w-full justify-center"
            >
              <Key className="w-4 h-4" />
              Manage SSH Keys
            </button>
          </div>
        </section>

        {/* Config Encryption */}
        <section>
          <h2 className="text-sm font-medium text-gray-600 dark:text-gray-300 mb-3">
            {t("settings.encryption")}
          </h2>
          <div className="bg-surface-2 rounded-lg p-4 border border-surface-3 space-y-3">
            <div className="flex items-center gap-2 text-sm">
              {isEncrypted ? (
                <>
                  <Lock className="w-4 h-4 text-vpn-on" />
                  <span className="text-vpn-on">Encrypted</span>
                </>
              ) : (
                <>
                  <LockOpen className="w-4 h-4 text-gray-500 dark:text-gray-400" />
                  <span className="text-gray-500 dark:text-gray-400">Not encrypted</span>
                </>
              )}
            </div>
            {isEncrypted ? (
              <button
                onClick={handleRemovePassword}
                disabled={encLoading}
                className="flex items-center gap-1 px-3 py-1.5 bg-red-500/20 hover:bg-red-500/30 text-red-400 rounded text-sm transition-colors disabled:opacity-50"
              >
                {encLoading && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                Remove Encryption
              </button>
            ) : (
              <div className="flex gap-2">
                <input
                  type="password"
                  placeholder="Set password..."
                  value={encPassword}
                  onChange={(e) => setEncPassword(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleSetPassword()}
                  className="flex-1 px-3 py-1.5 bg-surface-0 border border-surface-3 rounded text-sm text-gray-900 dark:text-white placeholder-gray-400 focus:outline-none focus:border-accent"
                />
                <button
                  onClick={handleSetPassword}
                  disabled={encLoading || !encPassword}
                  className="flex items-center gap-1 px-3 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-sm transition-colors disabled:opacity-50"
                >
                  {encLoading && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                  Enable
                </button>
              </div>
            )}
            {encStatus && (
              <p
                className={`text-xs ${encStatus.includes("Error") ? "text-red-400" : "text-vpn-on"}`}
              >
                {encStatus}
              </p>
            )}
          </div>
        </section>

        {/* About */}
        <section>
          <h2 className="text-sm font-medium text-gray-600 dark:text-gray-300 mb-3">
            {t("settings.about")}
          </h2>
          <div className="bg-surface-2 rounded-lg p-4 border border-surface-3 text-sm text-gray-500 dark:text-gray-400 space-y-1">
            <p>
              <span className="text-gray-900 dark:text-white font-medium">RemoteLab</span> v0.1.0
            </p>
            <p>{t("settings.aboutDesc")}</p>
            <p className="text-xs text-gray-400 dark:text-gray-500 mt-2">
              Built with Tauri 2.0 + React + Rust
            </p>
          </div>
        </section>
      </div>
    </div>
  );
}

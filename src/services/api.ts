import { invoke } from "@tauri-apps/api/core";
import type { Device, VpnStatus, RemoteFile } from "./types";

// VPN commands
export async function vpnConnect(configPath: string): Promise<VpnStatus> {
  return invoke("vpn_connect", { configPath });
}

export async function vpnDisconnect(): Promise<void> {
  return invoke("vpn_disconnect");
}

export async function vpnStatus(): Promise<VpnStatus> {
  return invoke("vpn_status");
}

export async function vpnImportConfig(path: string): Promise<string> {
  return invoke("vpn_import_config", { path });
}

export async function vpnHasConfig(): Promise<boolean> {
  return invoke("vpn_has_config");
}

// SSH commands
export async function sshOpen(
  sessionId: string,
  host: string,
  user: string,
  port?: number,
): Promise<void> {
  return invoke("ssh_open", { sessionId, host, user, port: port ?? null });
}

export async function sshWrite(
  sessionId: string,
  data: number[],
): Promise<void> {
  return invoke("ssh_write", { sessionId, data });
}

export async function sshResize(
  sessionId: string,
  cols: number,
  rows: number,
): Promise<void> {
  return invoke("ssh_resize", { sessionId, cols, rows });
}

export async function sshClose(sessionId: string): Promise<void> {
  return invoke("ssh_close", { sessionId });
}

// Device commands
interface DeviceWithStatus extends Device {
  online: boolean;
}

export async function listDevices(): Promise<DeviceWithStatus[]> {
  return invoke("list_devices");
}

export async function addDevice(
  name: string,
  vpnIp: string,
  sshUser: string,
  rustdeskId?: string,
  sshHost?: string,
  sshPort?: number,
): Promise<Device> {
  return invoke("add_device", {
    name,
    vpnIp,
    sshUser,
    rustdeskId: rustdeskId ?? null,
    sshHost: sshHost ?? null,
    sshPort: sshPort ?? null,
  });
}

export async function removeDevice(id: string): Promise<void> {
  return invoke("remove_device", { id });
}

export async function pingDevice(ip: string): Promise<boolean> {
  return invoke("ping_device", { ip });
}

// Config import/export
export async function exportConfig(): Promise<string> {
  return invoke("export_config");
}

export async function importConfig(jsonStr: string): Promise<void> {
  return invoke("import_config", { jsonStr });
}

// Desktop commands — smart auto-detect + embedded viewer
export interface DesktopConnection {
  mode: "sunshine" | "vnc";
  url: string;
  gpu_name: string;
  has_nvenc: boolean;
}

export interface GpuInfo {
  gpu_name: string;
  has_nvenc: boolean;
  has_display: boolean;
  driver_version: string;
}

/** Smart connect: auto-detect GPU → Sunshine or TurboVNC */
export async function desktopConnect(
  host: string,
  user: string,
  port?: number,
): Promise<DesktopConnection> {
  return invoke("desktop_connect", { host, user, port: port ?? null });
}

/** Detect GPU without connecting */
export async function detectGpu(
  host: string,
  user: string,
  port?: number,
): Promise<GpuInfo> {
  return invoke("detect_gpu", { host, user, port: port ?? null });
}

/** Legacy VNC connect */
export async function vncConnect(
  host: string,
  user: string,
  vncPort?: number,
): Promise<number> {
  return invoke("vnc_connect", { host, user, vncPort: vncPort ?? null });
}

export async function vncDisconnect(): Promise<void> {
  return invoke("vnc_disconnect");
}

export async function vncStatus(): Promise<boolean> {
  return invoke("vnc_status");
}

// File transfer commands
export async function sftpList(
  host: string,
  user: string,
  port: number | undefined,
  path: string,
): Promise<RemoteFile[]> {
  return invoke("sftp_list", { host, user, port: port ?? null, path });
}

export async function sftpUpload(
  host: string,
  user: string,
  port: number | undefined,
  localPath: string,
  remotePath: string,
): Promise<void> {
  return invoke("sftp_upload", { host, user, port: port ?? null, localPath, remotePath });
}

export async function sftpDownload(
  host: string,
  user: string,
  port: number | undefined,
  remotePath: string,
  localPath: string,
): Promise<void> {
  return invoke("sftp_download", { host, user, port: port ?? null, remotePath, localPath });
}

export async function sftpMkdir(
  host: string,
  user: string,
  port: number | undefined,
  path: string,
): Promise<void> {
  return invoke("sftp_mkdir", { host, user, port: port ?? null, path });
}

export async function sftpDelete(
  host: string,
  user: string,
  port: number | undefined,
  path: string,
): Promise<void> {
  return invoke("sftp_delete", { host, user, port: port ?? null, path });
}

// SSH Key Management
export interface SshKeyInfo {
  name: string;
  path: string;
  key_type: string;
  has_public: boolean;
  public_key: string | null;
  fingerprint: string | null;
}

export async function sshKeysList(): Promise<SshKeyInfo[]> {
  return invoke("ssh_keys_list");
}

export async function sshKeyGenerate(name: string, passphrase: string): Promise<SshKeyInfo> {
  return invoke("ssh_key_generate", { name, passphrase });
}

export async function sshKeyCopyToRemote(keyPath: string, host: string, user: string, port?: number): Promise<void> {
  return invoke("ssh_key_copy_to_remote", { keyPath, host, user, port: port ?? null });
}

// Config encryption
export async function configIsEncrypted(): Promise<boolean> {
  return invoke("config_is_encrypted");
}

export async function unlockConfig(password: string): Promise<void> {
  return invoke("unlock_config", { password });
}

export async function setConfigPassword(password: string): Promise<void> {
  return invoke("set_config_password", { password });
}

export async function removeConfigPassword(): Promise<void> {
  return invoke("remove_config_password");
}

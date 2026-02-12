export interface Device {
  id: string;
  name: string;
  vpn_ip: string;
  ssh_user: string;
  rustdesk_id?: string;
  ssh_host?: string;
  ssh_port?: number;
  online: boolean;
}

export interface VpnStatus {
  connected: boolean;
  local_ip?: string;
  gateway_ip?: string;
  latency_ms?: number;
  uptime_secs?: number;
  interface_name?: string;
}

export interface AppConfig {
  devices: Device[];
  wg_config_path?: string;
  default_ssh_user: string;
  rustdesk_server?: string;
  rustdesk_key?: string;
}

export interface SshSession {
  id: string;
  device_name: string;
  host: string;
  user: string;
}

export interface RemoteFile {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string;
  permissions: string;
}

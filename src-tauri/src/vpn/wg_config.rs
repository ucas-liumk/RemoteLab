use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WgConfig {
    pub interface: WgInterface,
    pub peer: WgPeer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WgInterface {
    pub private_key: String,
    pub address: String,
    pub dns: Option<String>,
    pub post_up: Option<String>,
    pub pre_down: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WgPeer {
    pub public_key: String,
    pub endpoint: String,
    pub allowed_ips: String,
    pub persistent_keepalive: Option<u32>,
}

impl WgConfig {
    pub fn parse(content: &str) -> Result<Self, String> {
        let mut private_key = String::new();
        let mut address = String::new();
        let mut dns = None;
        let mut post_up = None;
        let mut pre_down = None;
        let mut public_key = String::new();
        let mut endpoint = String::new();
        let mut allowed_ips = String::new();
        let mut persistent_keepalive = None;

        let mut in_peer = false;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line == "[Peer]" {
                in_peer = true;
                continue;
            }
            if line == "[Interface]" {
                in_peer = false;
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                if in_peer {
                    match key {
                        "PublicKey" => public_key = value.to_string(),
                        "Endpoint" => endpoint = value.to_string(),
                        "AllowedIPs" => allowed_ips = value.to_string(),
                        "PersistentKeepalive" => {
                            persistent_keepalive = value.parse().ok();
                        }
                        _ => {}
                    }
                } else {
                    match key {
                        "PrivateKey" => private_key = value.to_string(),
                        "Address" => address = value.to_string(),
                        "DNS" => dns = Some(value.to_string()),
                        "PostUp" => post_up = Some(value.to_string()),
                        "PreDown" => pre_down = Some(value.to_string()),
                        _ => {}
                    }
                }
            }
        }

        if private_key.is_empty() || address.is_empty() {
            return Err("Invalid WireGuard config: missing PrivateKey or Address".to_string());
        }

        Ok(WgConfig {
            interface: WgInterface {
                private_key,
                address,
                dns,
                post_up,
                pre_down,
            },
            peer: WgPeer {
                public_key,
                endpoint,
                allowed_ips,
                persistent_keepalive,
            },
        })
    }
}

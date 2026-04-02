use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_xray_api_port() -> u16 {
    10813
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Error,
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self::Disconnected
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub status: ConnectionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_server_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_profile_id: Option<String>,
    pub kill_switch_enabled: bool,
    pub uptime_secs: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            status: ConnectionStatus::Disconnected,
            active_server_id: None,
            active_profile_id: None,
            kill_switch_enabled: true,
            uptime_secs: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficEntry {
    pub timestamp_ms: u64,
    pub process_name: String,
    pub domain: String,
    pub destination: String,
    pub direction: TrafficDirection,
    pub bytes: u64,
    pub routed_via: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrafficDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub display_name: String,
    pub exe_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_base64: Option<String>,
    pub category: super::AppCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsSettings {
    pub proxy_dns: String,
    pub direct_dns: String,
}

impl Default for DnsSettings {
    fn default() -> Self {
        Self {
            proxy_dns: "https://8.8.8.8/dns-query".into(),
            direct_dns: "77.88.8.8".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub dns: DnsSettings,
    pub kill_switch: bool,
    pub auto_connect: bool,
    pub start_minimized: bool,
    pub auto_start: bool,
    pub theme: String,
    pub language: String,
    pub log_level: String,
    pub xray_socks_port: u16,
    pub xray_http_port: u16,
    #[serde(default = "default_xray_api_port")]
    pub xray_api_port: u16,
    pub system_proxy: bool,
    #[serde(default)]
    pub active_profile_id: Option<String>,
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        for (name, port) in [
            ("SOCKS", self.xray_socks_port),
            ("HTTP", self.xray_http_port),
            ("API", self.xray_api_port),
        ] {
            if port < 1024 || port > 65535 {
                return Err(format!("{name} port {port} out of range 1024-65535"));
            }
        }
        if self.xray_socks_port == self.xray_http_port
            || self.xray_socks_port == self.xray_api_port
            || self.xray_http_port == self.xray_api_port
        {
            return Err("Port numbers must be unique".into());
        }
        Ok(())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dns: DnsSettings::default(),
            kill_switch: true,
            auto_connect: false,
            start_minimized: false,
            auto_start: false,
            theme: "dark".into(),
            language: "ru".into(),
            log_level: "warn".into(),
            xray_socks_port: 10808,
            xray_http_port: 10809,
            xray_api_port: 10813,
            system_proxy: true,
            active_profile_id: None,
        }
    }
}

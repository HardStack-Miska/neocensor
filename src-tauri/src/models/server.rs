use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub uuid: String,
    pub flow: Option<String>,
    pub encryption: String,
    pub transport: TransportConfig,
    pub security: SecurityConfig,
}

impl ServerConfig {
    pub fn new_vless(
        name: String,
        address: String,
        port: u16,
        uuid: String,
        transport: TransportConfig,
        security: SecurityConfig,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            address,
            port,
            uuid,
            flow: None,
            encryption: "none".into(),
            transport,
            security,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.address.is_empty() {
            return Err("Server address is empty".into());
        }
        if self.port == 0 {
            return Err("Server port is 0".into());
        }
        if self.uuid.is_empty() {
            return Err("Server UUID is empty".into());
        }
        Ok(())
    }

    pub fn with_flow(mut self, flow: impl Into<String>) -> Self {
        self.flow = Some(flow.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TransportConfig {
    Tcp,
    Ws {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        host: Option<String>,
    },
    Grpc {
        service_name: String,
    },
    Xhttp {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        host: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
    },
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::Tcp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SecurityConfig {
    None,
    Tls {
        sni: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        fingerprint: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        alpn: Option<Vec<String>>,
    },
    Reality {
        sni: String,
        fingerprint: String,
        public_key: String,
        short_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        spider_x: Option<String>,
    },
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEntry {
    pub config: ServerConfig,
    pub source: ServerSource,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ping_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    pub favorite: bool,
    pub online: bool,
}

impl ServerEntry {
    pub fn from_config(config: ServerConfig, source: ServerSource) -> Self {
        let display_name = config.name.clone();
        Self {
            config,
            source,
            display_name,
            ping_ms: None,
            country: None,
            favorite: false,
            online: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerSource {
    Manual,
    Subscription(Uuid),
}

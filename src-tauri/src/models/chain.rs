#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ServerConfig;

/// A chain (multi-hop) configuration: bridge server -> exit server.
/// Used to bypass whitelist-mode blocking (RU bridge -> EU exit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub id: Uuid,
    pub name: String,
    pub bridge: ServerConfig,
    pub exit: ServerConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ping_ms: Option<u32>,
}

impl ChainConfig {
    pub fn new(name: impl Into<String>, bridge: ServerConfig, exit: ServerConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            bridge,
            exit,
            ping_ms: None,
        }
    }

    /// The bridge country code (first hop).
    pub fn bridge_country(&self) -> &str {
        // Derive from server name or address
        &self.bridge.name
    }

    /// The exit country code (final hop).
    pub fn exit_country(&self) -> &str {
        &self.exit.name
    }
}

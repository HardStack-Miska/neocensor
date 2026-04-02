use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ServerConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub servers: Vec<ServerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<DateTime<Utc>>,
    /// Update interval in seconds (default: 6 hours)
    pub update_interval_secs: u64,
    pub enabled: bool,
}

impl Subscription {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            url: url.into(),
            servers: Vec::new(),
            last_updated: None,
            update_interval_secs: 6 * 3600,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionFormat {
    Base64Uris,
    SingBoxJson,
    ClashYaml,
}

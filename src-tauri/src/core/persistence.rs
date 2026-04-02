use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

use crate::models::{AppRoute, Profile, ServerEntry, Settings, Subscription};

/// Simple JSON file persistence for app state.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json"))
    }

    fn load<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        let path = self.path(name);
        match std::fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => T::default(),
        }
    }

    fn save<T: Serialize + ?Sized>(&self, name: &str, data: &T) -> Result<()> {
        let path = self.path(name);
        let json = serde_json::to_string_pretty(data)?;
        // Atomic write: temp file + rename to prevent data loss on crash
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).context("failed to write temp store file")?;
        std::fs::rename(&tmp, &path).context("failed to rename temp store file")?;
        Ok(())
    }

    // ── Servers ──

    pub fn load_servers(&self) -> Vec<ServerEntry> {
        self.load("servers")
    }

    pub fn save_servers(&self, servers: &[ServerEntry]) -> Result<()> {
        self.save("servers", servers)
    }

    // ── Subscriptions ──

    pub fn load_subscriptions(&self) -> Vec<Subscription> {
        self.load("subscriptions")
    }

    pub fn save_subscriptions(&self, subs: &[Subscription]) -> Result<()> {
        self.save("subscriptions", subs)
    }

    // ── Routes ──

    pub fn load_routes(&self) -> Vec<AppRoute> {
        self.load("routes")
    }

    pub fn save_routes(&self, routes: &[AppRoute]) -> Result<()> {
        self.save("routes", routes)
    }

    // ── Profiles ──

    pub fn load_profiles(&self) -> Vec<Profile> {
        let profiles: Vec<Profile> = self.load("profiles");
        if profiles.is_empty() {
            Profile::default_profiles()
        } else {
            profiles
        }
    }

    pub fn save_profiles(&self, profiles: &[Profile]) -> Result<()> {
        self.save("profiles", profiles)
    }

    // ── Settings ──

    pub fn load_settings(&self) -> Settings {
        self.load("settings")
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.save("settings", settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AppRoute, RouteMode, SecurityConfig, ServerConfig, ServerEntry, ServerSource,
        TransportConfig,
    };
    use std::fs;

    fn temp_dir(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("neocensor_test_{suffix}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &std::path::Path) {
        let _ = fs::remove_dir_all(dir);
    }

    fn make_server_config(name: &str) -> ServerConfig {
        ServerConfig::new_vless(
            name.into(),
            "example.com".into(),
            443,
            "a1b2c3d4-e5f6-7890-abcd-ef1234567890".into(),
            TransportConfig::Tcp,
            SecurityConfig::None,
        )
    }

    #[test]
    fn save_and_load_servers() {
        let dir = temp_dir("servers");
        let store = Store::new(dir.clone());

        let config = make_server_config("Test-Server");
        let entry = ServerEntry::from_config(config, ServerSource::Manual);
        let servers = vec![entry];

        store.save_servers(&servers).unwrap();
        let loaded = store.load_servers();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].display_name, "Test-Server");
        assert_eq!(loaded[0].config.address, "example.com");
        assert_eq!(loaded[0].config.port, 443);
        assert!(!loaded[0].favorite);
        assert!(!loaded[0].online);

        cleanup(&dir);
    }

    #[test]
    fn save_and_load_multiple_servers() {
        let dir = temp_dir("multi_servers");
        let store = Store::new(dir.clone());

        let servers: Vec<ServerEntry> = (1..=5)
            .map(|i| {
                ServerEntry::from_config(
                    make_server_config(&format!("Server-{i}")),
                    ServerSource::Manual,
                )
            })
            .collect();

        store.save_servers(&servers).unwrap();
        let loaded = store.load_servers();
        assert_eq!(loaded.len(), 5);

        for (i, s) in loaded.iter().enumerate() {
            assert_eq!(s.display_name, format!("Server-{}", i + 1));
        }

        cleanup(&dir);
    }

    #[test]
    fn load_servers_from_nonexistent_file_returns_default() {
        let dir = temp_dir("nonexistent_servers");
        let store = Store::new(dir.clone());

        let loaded = store.load_servers();
        assert!(loaded.is_empty());

        cleanup(&dir);
    }

    #[test]
    fn load_servers_from_corrupted_json_returns_default() {
        let dir = temp_dir("corrupted_servers");
        fs::create_dir_all(&dir).unwrap();
        let store = Store::new(dir.clone());

        // Write invalid JSON
        fs::write(dir.join("servers.json"), "{ this is not valid json }}}").unwrap();

        let loaded = store.load_servers();
        assert!(loaded.is_empty());

        cleanup(&dir);
    }

    #[test]
    fn save_and_load_settings() {
        let dir = temp_dir("settings");
        let store = Store::new(dir.clone());

        let mut settings = Settings::default();
        settings.theme = "light".into();
        settings.language = "en".into();
        settings.kill_switch = false;
        settings.auto_connect = true;
        settings.xray_socks_port = 12345;
        settings.xray_http_port = 12346;

        store.save_settings(&settings).unwrap();
        let loaded = store.load_settings();

        assert_eq!(loaded.theme, "light");
        assert_eq!(loaded.language, "en");
        assert!(!loaded.kill_switch);
        assert!(loaded.auto_connect);
        assert_eq!(loaded.xray_socks_port, 12345);
        assert_eq!(loaded.xray_http_port, 12346);

        cleanup(&dir);
    }

    #[test]
    fn load_settings_nonexistent_returns_default() {
        let dir = temp_dir("nonexistent_settings");
        let store = Store::new(dir.clone());

        let loaded = store.load_settings();
        assert_eq!(loaded.theme, "dark");
        assert_eq!(loaded.language, "ru");
        assert!(loaded.kill_switch);
        assert!(!loaded.auto_connect);

        cleanup(&dir);
    }

    #[test]
    fn load_settings_corrupted_json_returns_default() {
        let dir = temp_dir("corrupted_settings");
        fs::create_dir_all(&dir).unwrap();
        let store = Store::new(dir.clone());

        fs::write(dir.join("settings.json"), "not json at all!!!").unwrap();

        let loaded = store.load_settings();
        // Should return default values
        assert_eq!(loaded.theme, "dark");
        assert!(loaded.kill_switch);

        cleanup(&dir);
    }

    #[test]
    fn save_and_load_routes() {
        let dir = temp_dir("routes");
        let store = Store::new(dir.clone());

        let routes = vec![
            AppRoute::new("chrome.exe", RouteMode::Proxy),
            AppRoute::new("steam.exe", RouteMode::Direct),
        ];

        store.save_routes(&routes).unwrap();
        let loaded = store.load_routes();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].process_name, "chrome.exe");
        assert_eq!(loaded[0].display_name, "chrome");
        assert_eq!(loaded[1].process_name, "steam.exe");

        cleanup(&dir);
    }

    #[test]
    fn load_profiles_empty_returns_defaults() {
        let dir = temp_dir("profiles_default");
        let store = Store::new(dir.clone());

        let loaded = store.load_profiles();
        // Should return default profiles
        assert!(!loaded.is_empty());
        let ids: Vec<&str> = loaded.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"gaming"));
        assert!(ids.contains(&"work"));
        assert!(ids.contains(&"smart"));
        assert!(ids.contains(&"full_vpn"));

        cleanup(&dir);
    }

    #[test]
    fn save_and_load_profiles() {
        let dir = temp_dir("profiles_save");
        let store = Store::new(dir.clone());

        let profiles = vec![Profile {
            id: "custom".into(),
            name: "Custom Profile".into(),
            icon: "star".into(),
            routes: vec![],
            default_mode: RouteMode::Proxy,
        }];

        store.save_profiles(&profiles).unwrap();
        let loaded = store.load_profiles();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "custom");
        assert_eq!(loaded[0].name, "Custom Profile");

        cleanup(&dir);
    }

    #[test]
    fn save_and_load_subscriptions() {
        let dir = temp_dir("subscriptions");
        let store = Store::new(dir.clone());

        let sub = Subscription::new("https://example.com/sub");
        let subs = vec![sub];

        store.save_subscriptions(&subs).unwrap();
        let loaded = store.load_subscriptions();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].url, "https://example.com/sub");
        assert!(loaded[0].enabled);

        cleanup(&dir);
    }

    #[test]
    fn overwrite_existing_data() {
        let dir = temp_dir("overwrite");
        let store = Store::new(dir.clone());

        // Save initial data
        let servers1 = vec![ServerEntry::from_config(
            make_server_config("First"),
            ServerSource::Manual,
        )];
        store.save_servers(&servers1).unwrap();

        // Overwrite with different data
        let servers2 = vec![
            ServerEntry::from_config(make_server_config("Second"), ServerSource::Manual),
            ServerEntry::from_config(make_server_config("Third"), ServerSource::Manual),
        ];
        store.save_servers(&servers2).unwrap();

        let loaded = store.load_servers();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].display_name, "Second");
        assert_eq!(loaded[1].display_name, "Third");

        cleanup(&dir);
    }
}

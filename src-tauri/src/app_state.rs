use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{broadcast, Mutex};

use crate::core::pac_server::PacServer;
use crate::core::persistence::Store;
use crate::core::process_monitor::ProcessMonitor;
use crate::core::traffic::ConnectionEvent;
use crate::core::wfp::WfpManager;
use crate::core::xray::XrayManager;
use crate::models::{AppRoute, AppState, Profile, ServerEntry, Settings, Subscription};

const MAX_CONNECTIONS: usize = 300;

/// Central application state managed by Tauri.
///
/// ## Mutex Lock Order (to prevent deadlocks)
///
/// When acquiring multiple locks, always follow this order:
///  1. settings
///  2. profiles
///  3. app_state
///  4. server_store
///  5. app_routes
///  6. subscriptions
///  7. wfp
///  8. process_monitor
///  9. pac_server
/// 10. connections
///
/// Clone + drop early locks before acquiring later ones when possible.
pub struct ManagedState {
    pub app_state: Mutex<AppState>,
    pub server_store: Mutex<Vec<ServerEntry>>,
    pub app_routes: Mutex<Vec<AppRoute>>,
    pub subscriptions: Mutex<Vec<Subscription>>,
    pub profiles: Mutex<Vec<Profile>>,
    pub settings: Mutex<Settings>,
    pub xray: XrayManager,
    pub process_monitor: Mutex<ProcessMonitor>,
    pub store: Store,
    pub log_sender: broadcast::Sender<String>,
    /// WFP per-process routing manager.
    pub wfp: Mutex<WfpManager>,
    /// PAC file server for proxy auto-config.
    pub pac_server: Mutex<PacServer>,
    /// Live parsed connections from xray logs.
    pub connections: Mutex<Vec<ConnectionEvent>>,
    /// Monotonic connection counter for unique IDs.
    pub conn_counter: AtomicU64,
}

impl ManagedState {
    pub fn new(
        xray: XrayManager,
        store: Store,
        log_sender: broadcast::Sender<String>,
    ) -> Self {
        let servers = store.load_servers();
        let routes = store.load_routes();
        let subscriptions = store.load_subscriptions();
        let profiles = store.load_profiles();
        let settings = store.load_settings();

        tracing::info!(
            "loaded {} servers, {} routes, {} subscriptions from disk",
            servers.len(),
            routes.len(),
            subscriptions.len()
        );

        // Restore active profile from settings
        let mut initial_state = AppState::default();
        initial_state.active_profile_id = settings.active_profile_id.clone();

        Self {
            app_state: Mutex::new(initial_state),
            server_store: Mutex::new(servers),
            app_routes: Mutex::new(routes),
            subscriptions: Mutex::new(subscriptions),
            profiles: Mutex::new(profiles),
            settings: Mutex::new(settings),
            xray,
            wfp: Mutex::new(WfpManager::new()),
            pac_server: Mutex::new(PacServer::new()),
            process_monitor: Mutex::new(ProcessMonitor::new()),
            store,
            log_sender,
            connections: Mutex::new(Vec::new()),
            conn_counter: AtomicU64::new(0),
        }
    }

    /// Add a parsed connection event, keeping the list bounded.
    pub async fn push_connection(&self, event: ConnectionEvent) {
        let mut conns = self.connections.lock().await;
        conns.push(event);
        let len = conns.len();
        if len > MAX_CONNECTIONS {
            conns.drain(0..len - MAX_CONNECTIONS);
        }
    }

    /// Get next unique connection ID.
    pub fn next_conn_id(&self) -> u64 {
        self.conn_counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Clear connections (on disconnect).
    pub async fn clear_connections(&self) {
        self.connections.lock().await.clear();
        self.conn_counter.store(0, Ordering::Relaxed);
    }

    pub async fn persist_servers(&self) {
        let servers = self.server_store.lock().await;
        if let Err(e) = self.store.save_servers(&servers) {
            tracing::error!("failed to save servers: {e}");
        }
    }

    pub async fn persist_routes(&self) {
        let routes = self.app_routes.lock().await;
        if let Err(e) = self.store.save_routes(&routes) {
            tracing::error!("failed to save routes: {e}");
        }
    }

    pub async fn persist_subscriptions(&self) {
        let subs = self.subscriptions.lock().await;
        if let Err(e) = self.store.save_subscriptions(&subs) {
            tracing::error!("failed to save subscriptions: {e}");
        }
    }

    pub async fn persist_settings(&self) {
        let settings = self.settings.lock().await;
        if let Err(e) = self.store.save_settings(&settings) {
            tracing::error!("failed to save settings: {e}");
        }
    }
}

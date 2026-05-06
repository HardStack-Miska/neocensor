use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::async_runtime::JoinHandle as TauriJoinHandle;
use tokio::sync::{broadcast, Mutex};

use crate::core::persistence::Store;
use crate::core::process_monitor::ProcessMonitor;
use crate::core::singbox::SingboxManager;
use crate::core::traffic::ConnectionEvent;
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
///  7. process_monitor
///  8. connections
///
/// Clone + drop early locks before acquiring later ones when possible.
pub struct ManagedState {
    pub app_state: Mutex<AppState>,
    pub server_store: Mutex<Vec<ServerEntry>>,
    pub app_routes: Mutex<Vec<AppRoute>>,
    pub subscriptions: Mutex<Vec<Subscription>>,
    pub profiles: Mutex<Vec<Profile>>,
    pub settings: Mutex<Settings>,
    pub singbox: SingboxManager,
    pub process_monitor: Mutex<ProcessMonitor>,
    pub store: Store,
    pub log_sender: broadcast::Sender<String>,
    /// Live parsed connections from sing-box logs.
    pub connections: Mutex<Vec<ConnectionEvent>>,
    /// Monotonic connection counter for unique IDs.
    pub conn_counter: AtomicU64,
    /// Whether the Windows system proxy was set by us (and thus needs unsetting).
    pub system_proxy_set: AtomicBool,
    /// Serializes subscription refresh operations (prevents double-refresh races).
    pub subscription_refresh_lock: Mutex<()>,
    /// Background task handles for clean shutdown.
    pub background_tasks: std::sync::Mutex<Vec<TauriJoinHandle<()>>>,
    /// Top-level lock for connect/disconnect commands. Prevents two concurrent
    /// `connect` invocations from racing on `active_server_id` / sing-box state.
    pub connect_lock: Mutex<()>,
    /// True while a transient sing-box restart is in flight (e.g. route change).
    /// Watchdog must NOT treat is_alive==false as a crash during this window.
    pub transition_in_progress: AtomicBool,
}

impl ManagedState {
    pub fn new(
        singbox: SingboxManager,
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
            singbox,
            process_monitor: Mutex::new(ProcessMonitor::new()),
            store,
            log_sender,
            connections: Mutex::new(Vec::new()),
            conn_counter: AtomicU64::new(0),
            system_proxy_set: AtomicBool::new(false),
            subscription_refresh_lock: Mutex::new(()),
            background_tasks: std::sync::Mutex::new(Vec::new()),
            connect_lock: Mutex::new(()),
            transition_in_progress: AtomicBool::new(false),
        }
    }

    /// Register a background task so it can be aborted on shutdown.
    /// Synchronous — std::sync::Mutex is fine since we never hold across await.
    pub fn register_task(&self, handle: TauriJoinHandle<()>) {
        if let Ok(mut guard) = self.background_tasks.lock() {
            guard.push(handle);
        }
    }

    /// Abort all registered background tasks. Called from the Exit handler.
    pub fn abort_background_tasks(&self) {
        let tasks = match self.background_tasks.lock() {
            Ok(mut g) => std::mem::take(&mut *g),
            Err(_) => return,
        };
        for task in tasks {
            task.abort();
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

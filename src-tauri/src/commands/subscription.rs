use tauri::State;

use crate::app_state::ManagedState;
use crate::models::{ServerEntry, ServerSource, Subscription};
use crate::parsers::subscription::parse_subscription;

#[tauri::command]
pub async fn get_subscriptions(
    state: State<'_, ManagedState>,
) -> Result<Vec<Subscription>, String> {
    let subs = state.subscriptions.lock().await;
    Ok(subs.clone())
}

#[tauri::command]
pub async fn add_subscription(
    state: State<'_, ManagedState>,
    url: String,
    name: Option<String>,
) -> Result<Subscription, String> {
    // Fetch subscription content
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("failed to fetch subscription: {e}"))?;

    let body = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {e}"))?;

    let servers = parse_subscription(&body).map_err(|e| e.to_string())?;

    let mut sub = Subscription::new(&url);
    sub.name = name.unwrap_or_else(|| format!("Subscription ({})", servers.len()));
    sub.servers = servers;
    sub.last_updated = Some(chrono::Utc::now());

    // Add server entries
    let sub_id = sub.id;
    let entries: Vec<ServerEntry> = sub
        .servers
        .iter()
        .map(|config| ServerEntry::from_config(config.clone(), ServerSource::Subscription(sub_id)))
        .collect();

    {
        let mut store = state.server_store.lock().await;
        store.extend(entries);
    }

    {
        let mut subs = state.subscriptions.lock().await;
        subs.push(sub.clone());
    }

    Ok(sub)
}

#[tauri::command]
pub async fn refresh_subscription(
    state: State<'_, ManagedState>,
    sub_id: String,
) -> Result<usize, String> {
    let id: uuid::Uuid = sub_id.parse().map_err(|e: uuid::Error| e.to_string())?;

    let url = {
        let subs = state.subscriptions.lock().await;
        subs.iter()
            .find(|s| s.id == id)
            .map(|s| s.url.clone())
            .ok_or("subscription not found".to_string())?
    };

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let body = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("failed to fetch: {e}"))?
        .text()
        .await
        .map_err(|e| format!("failed to read: {e}"))?;

    let new_servers = parse_subscription(&body).map_err(|e| e.to_string())?;
    let count = new_servers.len();

    // Lock order: server_store (4) before subscriptions (6)
    // Replace server entries for this subscription
    {
        let mut store = state.server_store.lock().await;
        store.retain(|s| !matches!(&s.source, ServerSource::Subscription(sid) if *sid == id));

        let entries: Vec<ServerEntry> = new_servers
            .clone()
            .into_iter()
            .map(|config| ServerEntry::from_config(config, ServerSource::Subscription(id)))
            .collect();
        store.extend(entries);
    }

    // Update subscription
    {
        let mut subs = state.subscriptions.lock().await;
        if let Some(sub) = subs.iter_mut().find(|s| s.id == id) {
            sub.servers = new_servers;
            sub.last_updated = Some(chrono::Utc::now());
        }
    }

    state.persist_servers().await;
    state.persist_subscriptions().await;

    Ok(count)
}

#[tauri::command]
pub async fn remove_subscription(
    state: State<'_, ManagedState>,
    sub_id: String,
) -> Result<(), String> {
    let id: uuid::Uuid = sub_id.parse().map_err(|e: uuid::Error| e.to_string())?;

    // Lock order: server_store (4) before subscriptions (6)
    {
        let mut store = state.server_store.lock().await;
        store.retain(|s| !matches!(&s.source, ServerSource::Subscription(sid) if *sid == id));
    }
    {
        let mut subs = state.subscriptions.lock().await;
        subs.retain(|s| s.id != id);
    }

    state.persist_servers().await;
    state.persist_subscriptions().await;

    Ok(())
}

#[tauri::command]
pub async fn refresh_all_subscriptions(
    state: State<'_, ManagedState>,
) -> Result<usize, String> {
    let sub_ids: Vec<uuid::Uuid> = {
        let subs = state.subscriptions.lock().await;
        subs.iter().filter(|s| s.enabled).map(|s| s.id).collect()
    };

    let mut total = 0;
    for id in sub_ids {
        match refresh_subscription_inner(&state, id).await {
            Ok(count) => total += count,
            Err(e) => {
                tracing::warn!("failed to refresh subscription {id}: {e}");
            }
        }
    }

    Ok(total)
}

async fn refresh_subscription_inner(
    state: &ManagedState,
    id: uuid::Uuid,
) -> Result<usize, String> {
    let url = {
        let subs = state.subscriptions.lock().await;
        subs.iter()
            .find(|s| s.id == id)
            .map(|s| s.url.clone())
            .ok_or("subscription not found".to_string())?
    };

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let body = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("failed to fetch: {e}"))?
        .text()
        .await
        .map_err(|e| format!("failed to read: {e}"))?;

    let new_servers = parse_subscription(&body).map_err(|e| e.to_string())?;
    let count = new_servers.len();

    // Lock order: server_store (4) before subscriptions (6)
    {
        let mut store = state.server_store.lock().await;
        store.retain(|s| !matches!(&s.source, ServerSource::Subscription(sid) if *sid == id));
        let entries: Vec<ServerEntry> = new_servers
            .clone()
            .into_iter()
            .map(|config| ServerEntry::from_config(config, ServerSource::Subscription(id)))
            .collect();
        store.extend(entries);
    }
    {
        let mut subs = state.subscriptions.lock().await;
        if let Some(sub) = subs.iter_mut().find(|s| s.id == id) {
            sub.servers = new_servers;
            sub.last_updated = Some(chrono::Utc::now());
        }
    }

    state.persist_servers().await;
    state.persist_subscriptions().await;

    Ok(count)
}

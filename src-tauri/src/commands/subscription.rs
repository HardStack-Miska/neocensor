use std::collections::HashMap;
use std::net::IpAddr;

use futures::StreamExt;
use tauri::State;

use crate::app_state::ManagedState;
use crate::models::{ServerConfig, ServerEntry, ServerSource, Subscription};
use crate::parsers::subscription::parse_subscription;

const SUBSCRIPTION_USER_AGENT: &str = concat!("NeoCensor/", env!("CARGO_PKG_VERSION"));
const MAX_SUBSCRIPTION_BODY: usize = 4 * 1024 * 1024; // 4 MiB

/// Canonicalize URL: lowercase scheme/host, strip default port, normalize path.
fn canonicalize_url(u: &url::Url) -> String {
    // url::Url already lowercases scheme and host on parse.
    // as_str() preserves the rest. We just normalize a trailing slash on the
    // bare-root path so "https://x/" and "https://x" dedupe to the same key.
    let s = u.as_str();
    s.trim_end_matches('/').to_string()
}

/// Validate scheme + literal-IP / hostname syntax. Pure-sync, used by tests too.
fn validate_url_syntax(raw: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(raw).map_err(|e| format!("invalid URL: {e}"))?;

    let scheme = parsed.scheme();
    #[cfg(not(test))]
    if scheme != "https" {
        return Err(format!(
            "only https:// is allowed for subscriptions (got {scheme}); cleartext URLs leak credentials"
        ));
    }
    #[cfg(test)]
    if scheme != "https" && scheme != "http" {
        return Err(format!("only http(s):// schemes are allowed, got {scheme}"));
    }

    let host = parsed.host_str().ok_or("URL has no host")?;
    let lower = host.to_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return Err("localhost subscriptions are not allowed".to_string());
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        if is_private_ip(&ip) {
            return Err(format!("private/internal IP {ip} not allowed for subscriptions"));
        }
    }
    Ok(parsed)
}

/// Full validation: syntax + DNS resolve. Defends against DNS rebinding by
/// rejecting hostnames that resolve to any private/loopback IP.
async fn validate_subscription_url(raw: &str) -> Result<url::Url, String> {
    let parsed = validate_url_syntax(raw)?;
    let host = parsed.host_str().ok_or("URL has no host")?.to_lowercase();

    // Skip DNS for already-validated literal IPs
    if host.parse::<IpAddr>().is_ok() {
        return Ok(parsed);
    }

    let port = parsed.port_or_known_default().unwrap_or(443);
    let lookup = tokio::net::lookup_host(format!("{host}:{port}"))
        .await
        .map_err(|e| format!("failed to resolve {host}: {e}"))?;
    let mut any = false;
    for sa in lookup {
        any = true;
        if is_private_ip(&sa.ip()) {
            return Err(format!(
                "{host} resolves to private/internal IP {} — refused as SSRF risk",
                sa.ip()
            ));
        }
    }
    if !any {
        return Err(format!("no addresses resolved for {host}"));
    }
    Ok(parsed)
}

fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.octets()[0] == 0
                || v4.octets()[0] == 169
                // CGNAT 100.64.0.0/10
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 0x40)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // Unique local fc00::/7
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // Link-local fe80::/10
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // IPv4-mapped (::ffff:0:0/96) — recurse on the embedded IPv4
                || v6
                    .to_ipv4_mapped()
                    .map(|v4| is_private_ip(&IpAddr::V4(v4)))
                    .unwrap_or(false)
        }
    }
}

/// Fetch subscription body. Validates URL with DNS resolve, builds a reqwest Client
/// that pins the resolved IP (no second DNS lookup → no rebinding), uses a custom
/// redirect policy that re-validates each hop, and streams the body with an
/// incremental size cap.
async fn fetch_subscription_body(url: &str) -> Result<String, String> {
    let parsed = validate_subscription_url(url).await?;
    let host = parsed.host_str().ok_or("URL has no host")?.to_lowercase();
    let port = parsed.port_or_known_default().unwrap_or(443);

    // Resolve and pick the first usable address (already validated above).
    let resolved: std::net::SocketAddr = tokio::net::lookup_host(format!("{host}:{port}"))
        .await
        .map_err(|e| format!("failed to resolve: {e}"))?
        .find(|sa| !is_private_ip(&sa.ip()))
        .ok_or("no public address resolved")?;

    let client = reqwest::Client::builder()
        .user_agent(SUBSCRIPTION_USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .resolve(&host, resolved)
        // Custom redirect policy — validates each hop's URL before following.
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 {
                return attempt.error("too many redirects");
            }
            let next = attempt.url();
            // Block scheme downgrade and non-http(s)
            #[cfg(not(test))]
            if next.scheme() != "https" {
                return attempt.error("redirect to non-https not allowed");
            }
            // Reject literal-IP redirects to private ranges
            if let Some(host) = next.host_str() {
                let lower = host.to_lowercase();
                if lower == "localhost" || lower.ends_with(".localhost") {
                    return attempt.error("redirect to localhost not allowed");
                }
                if let Ok(ip) = lower.parse::<IpAddr>() {
                    if is_private_ip(&ip) {
                        return attempt.error("redirect to private IP not allowed");
                    }
                }
            }
            attempt.follow()
        }))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(parsed.as_str())
        .send()
        .await
        .map_err(|e| format!("failed to fetch subscription: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("subscription fetch failed: HTTP {}", resp.status()));
    }

    // Reject early if Content-Length declares oversized body
    if let Some(cl) = resp.content_length() {
        if cl as usize > MAX_SUBSCRIPTION_BODY {
            return Err(format!(
                "subscription body too large (declared {cl} bytes, max {MAX_SUBSCRIPTION_BODY})"
            ));
        }
    }

    // Stream with incremental cap so a hostile server can't OOM us by chunked transfer.
    let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("body read error: {e}"))?;
        if buf.len() + chunk.len() > MAX_SUBSCRIPTION_BODY {
            return Err(format!(
                "subscription body exceeded {MAX_SUBSCRIPTION_BODY} bytes"
            ));
        }
        buf.extend_from_slice(&chunk);
    }

    String::from_utf8(buf).map_err(|e| format!("subscription is not valid UTF-8: {e}"))
}

/// Stable identity tuple for matching servers across refreshes.
/// All components are lowercased so case-only changes from upstream don't lose identity.
fn server_identity(c: &ServerConfig) -> (String, u16, String) {
    (c.address.to_lowercase(), c.port, c.uuid.to_ascii_lowercase())
}

/// Replace this subscription's servers in the store while preserving stable identity:
/// matching entries (by host:port:uuid) keep their `id`, `favorite`, `ping_ms`, etc.
fn merge_subscription_servers(
    store: &mut Vec<ServerEntry>,
    sub_id: uuid::Uuid,
    new_servers: &[ServerConfig],
) {
    let mut existing: HashMap<(String, u16, String), ServerEntry> = HashMap::new();
    store.retain(|s| match &s.source {
        ServerSource::Subscription(sid) if *sid == sub_id => {
            let key = server_identity(&s.config);
            existing.insert(key, s.clone());
            false
        }
        _ => true,
    });

    for new_config in new_servers {
        let key = server_identity(new_config);
        if let Some(mut old_entry) = existing.remove(&key) {
            // Preserve identity-bearing fields and runtime state (favorite, ping_ms,
            // online for active connection). Only update mutable upstream metadata.
            old_entry.config.name = new_config.name.clone();
            old_entry.config.flow = new_config.flow.clone();
            old_entry.config.encryption = new_config.encryption.clone();
            old_entry.config.transport = new_config.transport.clone();
            old_entry.config.security = new_config.security.clone();
            old_entry.display_name = new_config.name.clone();
            // Preserve old_entry.online (don't overwrite — active connection state)
            store.push(old_entry);
        } else {
            store.push(ServerEntry::from_config(
                new_config.clone(),
                ServerSource::Subscription(sub_id),
            ));
        }
    }
}

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
    // Hold the refresh lock for the entire add — also prevents two concurrent
    // add_subscription calls from racing on the dedup check.
    let _refresh_guard = state.subscription_refresh_lock.lock().await;

    let parsed = validate_subscription_url(&url).await?;
    let canonical_url = canonicalize_url(&parsed);

    // Reject duplicates by canonical URL
    {
        let subs = state.subscriptions.lock().await;
        if subs.iter().any(|s| canonicalize_url_str(&s.url) == canonical_url) {
            return Err("subscription with this URL already exists".to_string());
        }
    }

    let body = fetch_subscription_body(&url).await?;
    let servers = parse_subscription(&body).map_err(|e| e.to_string())?;

    let mut sub = Subscription::new(&canonical_url);
    sub.name = name.unwrap_or_else(|| format!("Subscription ({})", servers.len()));
    sub.servers = servers.clone();
    sub.last_updated = Some(chrono::Utc::now());

    let sub_id = sub.id;

    // Lock order: server_store (4) before subscriptions (6)
    {
        let mut store = state.server_store.lock().await;
        merge_subscription_servers(&mut store, sub_id, &servers);
    }
    {
        let mut subs = state.subscriptions.lock().await;
        subs.push(sub.clone());
    }

    // Persist subscriptions FIRST so a crash between the two writes can't leave
    // orphaned servers tagged with a sub_id that has no Subscription record.
    state.persist_subscriptions().await;
    state.persist_servers().await;

    Ok(sub)
}

fn canonicalize_url_str(s: &str) -> String {
    url::Url::parse(s)
        .map(|u| canonicalize_url(&u))
        .unwrap_or_else(|_| s.trim_end_matches('/').to_string())
}

#[tauri::command]
pub async fn refresh_subscription(
    state: State<'_, ManagedState>,
    sub_id: String,
) -> Result<usize, String> {
    let id: uuid::Uuid = sub_id.parse().map_err(|e: uuid::Error| e.to_string())?;
    refresh_subscription_internal(state.inner(), id).await
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

    // Persist subscriptions first to keep the same crash-ordering invariant as add.
    state.persist_subscriptions().await;
    state.persist_servers().await;

    Ok(())
}

#[tauri::command]
pub async fn refresh_all_subscriptions(
    state: State<'_, ManagedState>,
) -> Result<usize, String> {
    let _guard = state.subscription_refresh_lock.lock().await;

    let sub_ids: Vec<uuid::Uuid> = {
        let subs = state.subscriptions.lock().await;
        subs.iter().filter(|s| s.enabled).map(|s| s.id).collect()
    };

    let mut total = 0;
    for id in sub_ids {
        match refresh_subscription_inner_locked(state.inner(), id).await {
            Ok(count) => total += count,
            Err(e) => {
                tracing::warn!("failed to refresh subscription {id}: {e}");
            }
        }
    }

    Ok(total)
}

/// Public API for refreshing a single subscription (used by the auto-refresh scheduler).
pub async fn refresh_subscription_internal(
    state: &ManagedState,
    id: uuid::Uuid,
) -> Result<usize, String> {
    let _guard = state.subscription_refresh_lock.lock().await;
    refresh_subscription_inner_locked(state, id).await
}

/// Inner refresh — assumes the subscription_refresh_lock is already held by caller.
async fn refresh_subscription_inner_locked(
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

    let body = fetch_subscription_body(&url).await?;
    let new_servers = parse_subscription(&body).map_err(|e| e.to_string())?;
    let count = new_servers.len();

    // Lock order: server_store (4) before subscriptions (6)
    {
        let mut store = state.server_store.lock().await;
        merge_subscription_servers(&mut store, id, &new_servers);
    }
    {
        let mut subs = state.subscriptions.lock().await;
        if let Some(sub) = subs.iter_mut().find(|s| s.id == id) {
            sub.servers = new_servers;
            sub.last_updated = Some(chrono::Utc::now());
        }
    }

    state.persist_subscriptions().await;
    state.persist_servers().await;

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{SecurityConfig, ServerConfig, TransportConfig};

    fn srv(addr: &str, port: u16, uuid: &str, name: &str) -> ServerConfig {
        ServerConfig::new_vless(
            name.into(),
            addr.into(),
            port,
            uuid.into(),
            TransportConfig::Tcp,
            SecurityConfig::None,
        )
    }

    #[test]
    fn merge_preserves_id_and_favorite_for_matching_server() {
        let sub_id = uuid::Uuid::new_v4();
        let mut store: Vec<ServerEntry> = Vec::new();

        // Initial state: one server from this subscription, marked favorite
        let original = srv("a.example.com", 443, "uuid-1", "Old Name");
        let original_id = original.id;
        let mut entry = ServerEntry::from_config(original, ServerSource::Subscription(sub_id));
        entry.favorite = true;
        entry.ping_ms = Some(42);
        store.push(entry);

        // Refresh: same server (host:port:uuid match) but with new name
        let updated = srv("a.example.com", 443, "uuid-1", "New Name");
        merge_subscription_servers(&mut store, sub_id, &[updated]);

        assert_eq!(store.len(), 1);
        assert_eq!(store[0].config.id, original_id, "id must be preserved");
        assert!(store[0].favorite, "favorite must be preserved");
        assert_eq!(store[0].ping_ms, Some(42), "ping must be preserved");
        assert_eq!(store[0].config.name, "New Name");
    }

    #[test]
    fn merge_removes_server_no_longer_in_subscription() {
        let sub_id = uuid::Uuid::new_v4();
        let mut store: Vec<ServerEntry> = vec![
            ServerEntry::from_config(
                srv("a.example.com", 443, "uuid-1", "A"),
                ServerSource::Subscription(sub_id),
            ),
            ServerEntry::from_config(
                srv("b.example.com", 443, "uuid-2", "B"),
                ServerSource::Subscription(sub_id),
            ),
        ];

        // Refresh contains only server B
        merge_subscription_servers(&mut store, sub_id, &[srv("b.example.com", 443, "uuid-2", "B")]);

        assert_eq!(store.len(), 1);
        assert_eq!(store[0].config.uuid, "uuid-2");
    }

    #[test]
    fn merge_does_not_touch_other_subscriptions() {
        let sub_a = uuid::Uuid::new_v4();
        let sub_b = uuid::Uuid::new_v4();
        let mut store: Vec<ServerEntry> = vec![
            ServerEntry::from_config(
                srv("a.example.com", 443, "uuid-1", "A"),
                ServerSource::Subscription(sub_a),
            ),
            ServerEntry::from_config(
                srv("b.example.com", 443, "uuid-2", "B"),
                ServerSource::Subscription(sub_b),
            ),
            ServerEntry::from_config(
                srv("manual.example.com", 443, "uuid-3", "Manual"),
                ServerSource::Manual,
            ),
        ];

        merge_subscription_servers(&mut store, sub_a, &[]);

        // sub_a is wiped, sub_b and manual remain
        assert_eq!(store.len(), 2);
        assert!(store.iter().any(|s| s.config.uuid == "uuid-2"));
        assert!(store.iter().any(|s| s.config.uuid == "uuid-3"));
    }

    #[test]
    fn validate_url_rejects_unsupported_schemes() {
        assert!(validate_url_syntax("ftp://example.com").is_err());
        assert!(validate_url_syntax("file:///etc/passwd").is_err());
        assert!(validate_url_syntax("javascript:alert(1)").is_err());
    }

    #[test]
    fn validate_url_rejects_private_ips() {
        assert!(validate_url_syntax("http://10.0.0.1/sub").is_err());
        assert!(validate_url_syntax("http://192.168.1.1/sub").is_err());
        assert!(validate_url_syntax("http://127.0.0.1/sub").is_err());
        assert!(validate_url_syntax("http://169.254.169.254/sub").is_err());
        // CGNAT
        assert!(validate_url_syntax("http://100.64.1.1/sub").is_err());
        assert!(validate_url_syntax("http://100.127.255.255/sub").is_err());
    }

    #[test]
    fn validate_url_accepts_https() {
        assert!(validate_url_syntax("https://example.com/sub").is_ok());
        assert!(validate_url_syntax("https://sub.provider.net/path?token=x").is_ok());
    }

    #[test]
    fn validate_url_rejects_localhost() {
        assert!(validate_url_syntax("http://localhost/sub").is_err());
        assert!(validate_url_syntax("https://app.localhost/sub").is_err());
    }

    #[test]
    fn private_ipv4_mapped_blocked() {
        let v6: IpAddr = "::ffff:10.0.0.1".parse().unwrap();
        assert!(is_private_ip(&v6));
    }

    #[test]
    fn ipv6_link_local_blocked() {
        let v6: IpAddr = "fe80::1".parse().unwrap();
        assert!(is_private_ip(&v6));
    }

    #[test]
    fn canonicalize_strips_trailing_slash() {
        let a = canonicalize_url(&url::Url::parse("https://x.com/sub/").unwrap());
        let b = canonicalize_url(&url::Url::parse("https://x.com/sub").unwrap());
        assert_eq!(a, b);
    }
}

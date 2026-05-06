use std::collections::BTreeMap;

use anyhow::{bail, Result};
use serde_json::json;

use crate::models::{AppRoute, RouteMode, SecurityConfig, ServerConfig, Settings, TransportConfig};

pub struct ConfigGenerator;

impl ConfigGenerator {
    /// Generate sing-box config. `tun_mode` = true for TUN (needs admin), false for proxy-only.
    pub fn generate_singbox_config(
        server: &ServerConfig,
        settings: &Settings,
        routes: &[AppRoute],
        default_mode: RouteMode,
        tun_mode: bool,
    ) -> Result<serde_json::Value> {
        Self::generate_singbox_config_inner(
            server,
            settings,
            routes,
            default_mode,
            tun_mode,
            true,
        )
    }

    /// Same as `generate_singbox_config` but lets the caller disable geosite remote
    /// rule_sets — needed when GitHub is unreachable on cold-start and we don't
    /// want sing-box to hang fetching them.
    pub fn generate_singbox_config_no_geosite(
        server: &ServerConfig,
        settings: &Settings,
        routes: &[AppRoute],
        default_mode: RouteMode,
        tun_mode: bool,
    ) -> Result<serde_json::Value> {
        Self::generate_singbox_config_inner(
            server,
            settings,
            routes,
            default_mode,
            tun_mode,
            false,
        )
    }

    fn generate_singbox_config_inner(
        server: &ServerConfig,
        settings: &Settings,
        routes: &[AppRoute],
        default_mode: RouteMode,
        tun_mode: bool,
        allow_geosite: bool,
    ) -> Result<serde_json::Value> {
        let vless_outbound = build_vless_outbound(server)?;
        let needs_geosite = allow_geosite && uses_auto_mode(routes, default_mode);
        let route_rules = build_route_rules(routes, default_mode, tun_mode, needs_geosite);

        // Auto mode = geosite split: domains in geosite-ru go direct, rest via proxy.
        let default_outbound = match default_mode {
            RouteMode::Proxy => "proxy",
            // Auto's "default" path matches whatever didn't hit a direct/block rule,
            // which means it goes through the proxy by default.
            RouteMode::Auto => "proxy",
            RouteMode::Direct => "direct",
            RouteMode::Block => "block",
        };

        let mut inbounds = vec![];

        // TUN inbound — captures all system traffic (requires admin)
        if tun_mode {
            inbounds.push(json!({
                "type": "tun",
                "tag": "tun-in",
                "interface_name": "NeoCensor",
                "address": ["10.254.0.1/30"],
                "mtu": 9000,
                "auto_route": true,
                "strict_route": true,
                "stack": "system",
            }));
        }

        // Mixed inbound — HTTP+SOCKS5 proxy on localhost
        inbounds.push(json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "::",
            "listen_port": settings.mixed_port,
        }));

        let mut route_obj = json!({
            "auto_detect_interface": true,
            "default_domain_resolver": "direct-dns",
            "rules": route_rules,
            "final": default_outbound,
        });

        // Attach remote rule_sets for geosite-based Auto mode.
        if needs_geosite {
            route_obj["rule_set"] = json!([
                {
                    "tag": "geosite-private",
                    "type": "remote",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-private.srs",
                    "download_detour": "direct",
                    "update_interval": "168h",
                },
                {
                    "tag": "geosite-ru",
                    "type": "remote",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-category-ru.srs",
                    "download_detour": "direct",
                    "update_interval": "168h",
                },
                {
                    "tag": "geoip-ru",
                    "type": "remote",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-ru.srs",
                    "download_detour": "direct",
                    "update_interval": "168h",
                }
            ]);
        }

        // Place sing-box cache file alongside the config (absolute path) so writes
        // don't depend on sing-box's CWD (which is inherited from our spawn — usually
        // Program Files, where unprivileged writes fail).
        let cache_path = crate::utils::data_dir()
            .ok()
            .map(|d| d.join("singbox-cache.db").to_string_lossy().into_owned())
            .unwrap_or_else(|| "singbox-cache.db".to_string());

        let config = json!({
            "log": {
                "level": "info",
                "timestamp": true,
            },
            "experimental": {
                "clash_api": {
                    "external_controller": "127.0.0.1:9090",
                },
                "cache_file": {
                    "enabled": true,
                    "path": cache_path,
                }
            },
            "dns": {
                "servers": [
                    {
                        "tag": "proxy-dns",
                        "type": "https",
                        "server": "8.8.8.8",
                        "detour": "proxy",
                    },
                    {
                        "tag": "direct-dns",
                        "type": "udp",
                        "server": settings.dns.direct_dns,
                    },
                ],
                "final": "proxy-dns",
            },
            "inbounds": inbounds,
            "outbounds": [
                vless_outbound,
                { "type": "direct", "tag": "direct" },
                { "type": "block", "tag": "block" },
            ],
            "route": route_obj,
        });

        Ok(config)
    }
}

/// True if any route or the default mode uses Auto (geosite split).
fn uses_auto_mode(routes: &[AppRoute], default_mode: RouteMode) -> bool {
    matches!(default_mode, RouteMode::Auto)
        || routes.iter().any(|r| matches!(r.mode, RouteMode::Auto))
}

/// Build VLESS outbound from ServerConfig.
fn build_vless_outbound(server: &ServerConfig) -> Result<serde_json::Value> {
    let mut outbound = json!({
        "type": "vless",
        "tag": "proxy",
        "server": server.address,
        "server_port": server.port,
        "uuid": server.uuid,
    });

    if let Some(flow) = &server.flow {
        outbound["flow"] = json!(flow);
    }

    // Transport
    match &server.transport {
        TransportConfig::Tcp => {
            // No transport field needed for TCP
        }
        TransportConfig::Ws { path, host } => {
            let mut transport = json!({
                "type": "ws",
                "path": path,
            });
            if let Some(h) = host {
                transport["headers"] = json!({ "Host": h });
            }
            outbound["transport"] = transport;
        }
        TransportConfig::Grpc { service_name } => {
            outbound["transport"] = json!({
                "type": "grpc",
                "service_name": service_name,
            });
        }
        TransportConfig::Xhttp { .. } => {
            bail!("xhttp transport not supported with sing-box");
        }
    }

    // Security / TLS
    match &server.security {
        SecurityConfig::None => {
            // No TLS field
        }
        SecurityConfig::Tls {
            sni,
            fingerprint,
            alpn,
        } => {
            let fp = fingerprint.as_deref().unwrap_or("chrome");
            let mut tls = json!({
                "enabled": true,
                "server_name": sni,
                "utls": {
                    "enabled": true,
                    "fingerprint": fp,
                },
            });
            if let Some(alpn) = alpn {
                tls["alpn"] = json!(alpn);
            }
            outbound["tls"] = tls;
        }
        SecurityConfig::Reality {
            sni,
            fingerprint,
            public_key,
            short_id,
            ..
        } => {
            outbound["tls"] = json!({
                "enabled": true,
                "server_name": sni,
                "utls": {
                    "enabled": true,
                    "fingerprint": fingerprint,
                },
                "reality": {
                    "enabled": true,
                    "public_key": public_key,
                    "short_id": short_id,
                },
            });
        }
    }

    Ok(outbound)
}

/// Build route rules array including per-app process_name rules.
fn build_route_rules(
    routes: &[AppRoute],
    default_mode: RouteMode,
    tun_mode: bool,
    needs_geosite: bool,
) -> Vec<serde_json::Value> {
    let mut rules = vec![
        // Sniff all inbound traffic
        json!({ "action": "sniff" }),
        // DNS hijack
        json!({ "protocol": "dns", "action": "hijack-dns" }),
        // Private IPs go direct
        json!({ "ip_is_private": true, "outbound": "direct" }),
    ];

    // Per-app rules in TUN mode
    if tun_mode {
        // BTreeMap → deterministic iteration order (config diff stability)
        let mut grouped: BTreeMap<RouteMode, Vec<String>> = BTreeMap::new();
        for route in routes {
            grouped
                .entry(route.mode)
                .or_default()
                .push(route.process_name.clone());
        }
        // Also sort process names within each group for full determinism
        for v in grouped.values_mut() {
            v.sort();
        }

        // Auto-mode apps: emit geosite split rules (geo-ru direct, rest proxy) before
        // the catch-all proxy rule below.
        if let Some(auto_apps) = grouped.get(&RouteMode::Auto) {
            if needs_geosite && !auto_apps.is_empty() {
                rules.push(json!({
                    "process_name": auto_apps,
                    "rule_set": ["geosite-private", "geosite-ru", "geoip-ru"],
                    "outbound": "direct",
                }));
                rules.push(json!({
                    "process_name": auto_apps,
                    "outbound": "proxy",
                }));
            }
        }

        for (mode, process_names) in &grouped {
            if matches!(mode, RouteMode::Auto) {
                continue; // already handled
            }
            let outbound = match mode {
                RouteMode::Proxy => "proxy",
                RouteMode::Direct => "direct",
                RouteMode::Block => "block",
                RouteMode::Auto => unreachable!(),
            };
            rules.push(json!({
                "process_name": process_names,
                "outbound": outbound,
            }));
        }
    }

    // Default-mode geosite split: applies to everything not matched by per-app rules.
    if matches!(default_mode, RouteMode::Auto) && needs_geosite {
        rules.push(json!({
            "rule_set": ["geosite-private", "geosite-ru", "geoip-ru"],
            "outbound": "direct",
        }));
    }

    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;

    fn test_server() -> ServerConfig {
        ServerConfig::new_vless(
            "Test Server".into(),
            "example.com".into(),
            443,
            "test-uuid".into(),
            TransportConfig::Tcp,
            SecurityConfig::Reality {
                sni: "www.microsoft.com".into(),
                fingerprint: "chrome".into(),
                public_key: "testkey123".into(),
                short_id: "ab".into(),
                spider_x: None,
            },
        )
        .with_flow("xtls-rprx-vision")
    }

    #[test]
    fn generate_singbox_config_reality() {
        let server = test_server();
        let settings = Settings::default();
        let routes = vec![];
        let config = ConfigGenerator::generate_singbox_config(
            &server, &settings, &routes, RouteMode::Proxy, true,
        )
        .unwrap();

        let outbound = &config["outbounds"][0];
        assert_eq!(outbound["type"], "vless");
        assert_eq!(outbound["tls"]["reality"]["enabled"], true);
        assert_eq!(outbound["flow"], "xtls-rprx-vision");
        assert_eq!(outbound["server"], "example.com");
        assert_eq!(outbound["server_port"], 443);
        // TUN inbound
        assert_eq!(config["inbounds"][0]["type"], "tun");
        assert_eq!(config["inbounds"][0]["tag"], "tun-in");
        // Mixed inbound
        assert_eq!(config["inbounds"][1]["type"], "mixed");
        assert_eq!(config["inbounds"][1]["listen_port"], 2080);
    }

    #[test]
    fn generate_singbox_config_tls_default_fingerprint() {
        let server = ServerConfig::new_vless(
            "TLS Server".into(),
            "example.com".into(),
            443,
            "test-uuid".into(),
            TransportConfig::Tcp,
            SecurityConfig::Tls {
                sni: "example.com".into(),
                fingerprint: None,
                alpn: None,
            },
        );
        let settings = Settings::default();
        let config = ConfigGenerator::generate_singbox_config(
            &server, &settings, &[], RouteMode::Proxy, true,
        )
        .unwrap();

        // Should default to chrome fingerprint
        assert_eq!(
            config["outbounds"][0]["tls"]["utls"]["fingerprint"],
            "chrome"
        );
    }

    #[test]
    fn generate_singbox_config_ws_transport() {
        let server = ServerConfig::new_vless(
            "WS Server".into(),
            "example.com".into(),
            443,
            "test-uuid".into(),
            TransportConfig::Ws {
                path: "/ws".into(),
                host: Some("cdn.example.com".into()),
            },
            SecurityConfig::Tls {
                sni: "cdn.example.com".into(),
                fingerprint: Some("firefox".into()),
                alpn: Some(vec!["h2".into(), "http/1.1".into()]),
            },
        );
        let settings = Settings::default();
        let config = ConfigGenerator::generate_singbox_config(
            &server, &settings, &[], RouteMode::Proxy, true,
        )
        .unwrap();

        assert_eq!(config["outbounds"][0]["transport"]["type"], "ws");
        assert_eq!(config["outbounds"][0]["transport"]["path"], "/ws");
        assert_eq!(
            config["outbounds"][0]["transport"]["headers"]["Host"],
            "cdn.example.com"
        );
    }

    #[test]
    fn generate_singbox_config_xhttp_unsupported() {
        let server = ServerConfig::new_vless(
            "XHTTP Server".into(),
            "example.com".into(),
            443,
            "test-uuid".into(),
            TransportConfig::Xhttp {
                path: "/xhttp".into(),
                host: None,
                mode: None,
            },
            SecurityConfig::None,
        );
        let settings = Settings::default();
        let result = ConfigGenerator::generate_singbox_config(
            &server, &settings, &[], RouteMode::Proxy, true,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("xhttp"));
    }

    #[test]
    fn generate_singbox_config_with_routes() {
        let server = test_server();
        let settings = Settings::default();
        let routes = vec![
            AppRoute::new("chrome.exe", RouteMode::Proxy)
                .with_display_name("Chrome"),
            AppRoute::new("discord.exe", RouteMode::Proxy)
                .with_display_name("Discord"),
            AppRoute::new("steam.exe", RouteMode::Direct)
                .with_display_name("Steam"),
        ];
        let config = ConfigGenerator::generate_singbox_config(
            &server, &settings, &routes, RouteMode::Direct, true,
        )
        .unwrap();

        let route_rules = config["route"]["rules"].as_array().unwrap();
        // dns rule + private ip rule + per-app rules
        assert!(route_rules.len() >= 3);

        // Final route should be direct
        assert_eq!(config["route"]["final"], "direct");
    }

    #[test]
    fn generate_singbox_config_auto_mode_emits_geosite_rule_set() {
        let server = test_server();
        let settings = Settings::default();
        let config = ConfigGenerator::generate_singbox_config(
            &server, &settings, &[], RouteMode::Auto, true,
        )
        .unwrap();

        // Auto mode must emit rule_set entries for geosite split
        let rule_sets = config["route"]["rule_set"]
            .as_array()
            .expect("Auto mode must emit rule_set array");
        let tags: Vec<&str> = rule_sets
            .iter()
            .filter_map(|rs| rs["tag"].as_str())
            .collect();
        assert!(tags.contains(&"geosite-ru"));
        assert!(tags.contains(&"geoip-ru"));
        assert!(tags.contains(&"geosite-private"));

        // Default route is "proxy" — geosite rules above route Russian traffic direct,
        // anything else falls through to the proxy outbound.
        assert_eq!(config["route"]["final"], "proxy");

        // Inspect rules for the direct geosite rule
        let rules = config["route"]["rules"].as_array().unwrap();
        let has_geosite_direct = rules.iter().any(|r| {
            r["outbound"] == "direct"
                && r["rule_set"]
                    .as_array()
                    .map(|a| a.iter().any(|t| t == "geosite-ru"))
                    .unwrap_or(false)
        });
        assert!(has_geosite_direct, "Auto mode must route geosite-ru direct");
    }

    #[test]
    fn generate_singbox_config_proxy_mode_does_not_emit_rule_set() {
        let server = test_server();
        let settings = Settings::default();
        let config = ConfigGenerator::generate_singbox_config(
            &server, &settings, &[], RouteMode::Proxy, true,
        )
        .unwrap();

        // Proxy-only mode must NOT include geosite rule_sets (they cost startup latency)
        assert!(config["route"]["rule_set"].is_null());
    }

    #[test]
    fn generate_singbox_config_grpc_transport() {
        let server = ServerConfig::new_vless(
            "gRPC Server".into(),
            "example.com".into(),
            443,
            "test-uuid".into(),
            TransportConfig::Grpc {
                service_name: "myservice".into(),
            },
            SecurityConfig::Tls {
                sni: "example.com".into(),
                fingerprint: Some("chrome".into()),
                alpn: Some(vec!["h2".into()]),
            },
        );
        let settings = Settings::default();
        let config = ConfigGenerator::generate_singbox_config(
            &server, &settings, &[], RouteMode::Proxy, true,
        )
        .unwrap();

        assert_eq!(config["outbounds"][0]["transport"]["type"], "grpc");
        assert_eq!(
            config["outbounds"][0]["transport"]["service_name"],
            "myservice"
        );
    }
}

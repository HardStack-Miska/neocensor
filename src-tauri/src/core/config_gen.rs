use std::collections::HashMap;

use anyhow::{bail, Result};
use serde_json::json;

use crate::models::{AppRoute, RouteMode, SecurityConfig, ServerConfig, Settings, TransportConfig};

pub struct ConfigGenerator;

impl ConfigGenerator {
    /// Generate sing-box JSON config with TUN + mixed inbounds.
    pub fn generate_singbox_config(
        server: &ServerConfig,
        settings: &Settings,
        routes: &[AppRoute],
        default_mode: RouteMode,
    ) -> Result<serde_json::Value> {
        let vless_outbound = build_vless_outbound(server)?;
        let route_rules = build_route_rules(routes, default_mode);

        let default_outbound = match default_mode {
            RouteMode::Proxy | RouteMode::Auto => "proxy",
            RouteMode::Direct => "direct",
            RouteMode::Block => "block",
        };

        let config = json!({
            "log": {
                "level": "info",
                "timestamp": true,
            },
            "experimental": {
                "clash_api": {
                    "external_controller": "127.0.0.1:9090",
                },
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
            "inbounds": [
                {
                    "type": "tun",
                    "tag": "tun-in",
                    "interface_name": "NeoCensor",
                    "address": ["10.254.0.1/30"],
                    "mtu": 9000,
                    "auto_route": true,
                    "strict_route": true,
                    "stack": "system",
                },
                {
                    "type": "mixed",
                    "tag": "mixed-in",
                    "listen": "::",
                    "listen_port": settings.mixed_port,
                },
            ],
            "outbounds": [
                vless_outbound,
                { "type": "direct", "tag": "direct" },
                { "type": "block", "tag": "block" },
            ],
            "route": {
                "auto_detect_interface": true,
                "default_domain_resolver": "direct-dns",
                "rules": route_rules,
                "final": default_outbound,
            },
        });

        Ok(config)
    }
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
fn build_route_rules(routes: &[AppRoute], _default_mode: RouteMode) -> Vec<serde_json::Value> {
    let mut rules = vec![
        // Sniff all inbound traffic (replaces legacy "sniff": true in inbounds)
        json!({ "action": "sniff" }),
        // DNS hijack (replaces legacy dns outbound)
        json!({ "protocol": "dns", "action": "hijack-dns" }),
        // Private IPs go direct
        json!({ "ip_is_private": true, "outbound": "direct" }),
    ];

    // Group routes by mode
    let mut grouped: HashMap<RouteMode, Vec<String>> = HashMap::new();
    for route in routes {
        grouped
            .entry(route.mode)
            .or_default()
            .push(route.process_name.clone());
    }

    // Create one rule per mode with process_name array
    for (mode, process_names) in &grouped {
        let outbound = match mode {
            RouteMode::Proxy | RouteMode::Auto => "proxy",
            RouteMode::Direct => "direct",
            RouteMode::Block => "block",
        };
        rules.push(json!({
            "process_name": process_names,
            "outbound": outbound,
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
            &server, &settings, &routes, RouteMode::Proxy,
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
            &server, &settings, &[], RouteMode::Proxy,
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
            &server, &settings, &[], RouteMode::Proxy,
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
            &server, &settings, &[], RouteMode::Proxy,
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
            &server, &settings, &routes, RouteMode::Direct,
        )
        .unwrap();

        let route_rules = config["route"]["rules"].as_array().unwrap();
        // dns rule + private ip rule + per-app rules
        assert!(route_rules.len() >= 3);

        // Final route should be direct
        assert_eq!(config["route"]["final"], "direct");
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
            &server, &settings, &[], RouteMode::Proxy,
        )
        .unwrap();

        assert_eq!(config["outbounds"][0]["transport"]["type"], "grpc");
        assert_eq!(
            config["outbounds"][0]["transport"]["service_name"],
            "myservice"
        );
    }
}

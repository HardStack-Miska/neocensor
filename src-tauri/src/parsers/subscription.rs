use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose, Engine};

use crate::models::{ServerConfig, SubscriptionFormat};

use super::vless::parse_vless_uri;

/// Detect the format of a subscription response body.
pub fn detect_format(body: &str) -> SubscriptionFormat {
    let trimmed = body.trim();

    // Try JSON first
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
            return SubscriptionFormat::SingBoxJson;
        }
    }

    // Try YAML (Clash format)
    if trimmed.starts_with("proxies:") || trimmed.contains("\nproxies:") {
        return SubscriptionFormat::ClashYaml;
    }

    // Default: assume base64-encoded URI list
    SubscriptionFormat::Base64Uris
}

/// Parse a subscription response body into a list of server configs.
pub fn parse_subscription(body: &str) -> Result<Vec<ServerConfig>> {
    let format = detect_format(body);
    match format {
        SubscriptionFormat::Base64Uris => parse_base64_uris(body),
        SubscriptionFormat::SingBoxJson => parse_singbox_json(body),
        SubscriptionFormat::ClashYaml => parse_clash_yaml(body),
    }
}

/// Parse base64-encoded list of VLESS URIs (one per line).
fn parse_base64_uris(body: &str) -> Result<Vec<ServerConfig>> {
    let trimmed = body.trim();

    // Try base64 decode first
    let decoded = if trimmed.contains("://") {
        // Already plain text URIs
        trimmed.to_string()
    } else {
        // Remove whitespace/newlines that might break base64
        let clean: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
        let bytes = general_purpose::STANDARD
            .decode(&clean)
            .or_else(|_| general_purpose::URL_SAFE.decode(&clean))
            .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(&clean))
            .context("failed to decode base64 subscription")?;
        String::from_utf8(bytes).context("subscription is not valid UTF-8")?
    };

    let mut servers = Vec::new();
    for line in decoded.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        if line.starts_with("vless://") {
            match parse_vless_uri(line) {
                Ok(config) => servers.push(config),
                Err(e) => {
                    tracing::warn!("skipping invalid VLESS URI: {e}");
                }
            }
        }
        // TODO: support vmess://, trojan://, ss:// in the future
    }

    if servers.is_empty() {
        return Err(anyhow!("no valid servers found in subscription"));
    }

    Ok(servers)
}

/// Parse sing-box JSON format subscription.
fn parse_singbox_json(body: &str) -> Result<Vec<ServerConfig>> {
    let json: serde_json::Value =
        serde_json::from_str(body).context("invalid JSON in subscription")?;

    let outbounds = json
        .get("outbounds")
        .and_then(|o| o.as_array())
        .ok_or_else(|| anyhow!("missing outbounds array in sing-box JSON"))?;

    let mut servers = Vec::new();
    for outbound in outbounds {
        let ob_type = outbound
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("");

        if ob_type != "vless" {
            continue;
        }

        let config = parse_singbox_vless_outbound(outbound)?;
        servers.push(config);
    }

    if servers.is_empty() {
        return Err(anyhow!("no VLESS servers found in sing-box JSON"));
    }

    Ok(servers)
}

fn parse_singbox_vless_outbound(ob: &serde_json::Value) -> Result<ServerConfig> {
    use crate::models::{SecurityConfig, TransportConfig};

    let server = ob
        .get("server")
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow!("missing server field"))?;
    let port = ob
        .get("server_port")
        .and_then(|p| p.as_u64())
        .ok_or_else(|| anyhow!("missing server_port"))? as u16;
    let uuid = ob
        .get("uuid")
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow!("missing uuid"))?;
    let tag = ob
        .get("tag")
        .and_then(|t| t.as_str())
        .unwrap_or(server);
    let flow = ob.get("flow").and_then(|f| f.as_str());

    // Parse transport
    let transport = if let Some(tp) = ob.get("transport") {
        let tp_type = tp.get("type").and_then(|t| t.as_str()).unwrap_or("tcp");
        match tp_type {
            "ws" => TransportConfig::Ws {
                path: tp
                    .get("path")
                    .and_then(|p| p.as_str())
                    .unwrap_or("/")
                    .into(),
                host: tp.get("headers")
                    .and_then(|h| h.get("Host"))
                    .and_then(|h| h.as_str())
                    .map(String::from),
            },
            "grpc" => TransportConfig::Grpc {
                service_name: tp
                    .get("service_name")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .into(),
            },
            _ => TransportConfig::Tcp,
        }
    } else {
        TransportConfig::Tcp
    };

    // Parse TLS/Reality
    let security = if let Some(tls) = ob.get("tls") {
        let enabled = tls.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false);
        if !enabled {
            SecurityConfig::None
        } else if let Some(reality) = tls.get("reality") {
            let enabled = reality
                .get("enabled")
                .and_then(|e| e.as_bool())
                .unwrap_or(false);
            if enabled {
                SecurityConfig::Reality {
                    sni: tls
                        .get("server_name")
                        .and_then(|s| s.as_str())
                        .unwrap_or(server)
                        .into(),
                    fingerprint: tls
                        .get("utls")
                        .and_then(|u| u.get("fingerprint"))
                        .and_then(|f| f.as_str())
                        .unwrap_or("chrome")
                        .into(),
                    public_key: reality
                        .get("public_key")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .into(),
                    short_id: reality
                        .get("short_id")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .into(),
                    spider_x: None,
                }
            } else {
                SecurityConfig::Tls {
                    sni: tls
                        .get("server_name")
                        .and_then(|s| s.as_str())
                        .unwrap_or(server)
                        .into(),
                    fingerprint: tls
                        .get("utls")
                        .and_then(|u| u.get("fingerprint"))
                        .and_then(|f| f.as_str())
                        .map(String::from),
                    alpn: tls.get("alpn").and_then(|a| {
                        a.as_array().map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                    }),
                }
            }
        } else {
            SecurityConfig::Tls {
                sni: tls
                    .get("server_name")
                    .and_then(|s| s.as_str())
                    .unwrap_or(server)
                    .into(),
                fingerprint: None,
                alpn: None,
            }
        }
    } else {
        SecurityConfig::None
    };

    let mut config =
        ServerConfig::new_vless(tag.into(), server.into(), port, uuid.into(), transport, security);

    if let Some(f) = flow {
        if !f.is_empty() {
            config = config.with_flow(f);
        }
    }

    Ok(config)
}

/// Parse Clash/Mihomo YAML format subscription.
fn parse_clash_yaml(body: &str) -> Result<Vec<ServerConfig>> {
    use crate::models::{SecurityConfig, TransportConfig};

    let yaml: serde_yaml::Value =
        serde_yaml::from_str(body).context("invalid YAML in subscription")?;

    let proxies = yaml
        .get("proxies")
        .and_then(|p| p.as_sequence())
        .ok_or_else(|| anyhow!("missing proxies array in Clash YAML"))?;

    let mut servers = Vec::new();
    for proxy in proxies {
        let proxy_type = proxy
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("");

        if proxy_type != "vless" {
            continue;
        }

        let name = proxy
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("Unknown");
        let server = proxy
            .get("server")
            .and_then(|s| s.as_str())
            .ok_or_else(|| anyhow!("missing server in proxy"))?;
        let port = proxy
            .get("port")
            .and_then(|p| p.as_u64())
            .ok_or_else(|| anyhow!("missing port in proxy"))? as u16;
        let uuid = proxy
            .get("uuid")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow!("missing uuid in proxy"))?;
        let flow = proxy.get("flow").and_then(|f| f.as_str());

        // Parse transport
        let network = proxy
            .get("network")
            .and_then(|n| n.as_str())
            .unwrap_or("tcp");
        let transport = match network {
            "ws" => {
                let opts = proxy.get("ws-opts");
                TransportConfig::Ws {
                    path: opts
                        .and_then(|o| o.get("path"))
                        .and_then(|p| p.as_str())
                        .unwrap_or("/")
                        .into(),
                    host: opts
                        .and_then(|o| o.get("headers"))
                        .and_then(|h| h.get("Host"))
                        .and_then(|h| h.as_str())
                        .map(String::from),
                }
            }
            "grpc" => {
                let opts = proxy.get("grpc-opts");
                TransportConfig::Grpc {
                    service_name: opts
                        .and_then(|o| o.get("grpc-service-name"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .into(),
                }
            }
            _ => TransportConfig::Tcp,
        };

        // Parse security
        let tls = proxy
            .get("tls")
            .and_then(|t| t.as_bool())
            .unwrap_or(false);
        let security = if let Some(reality_opts) = proxy.get("reality-opts") {
            SecurityConfig::Reality {
                sni: proxy
                    .get("servername")
                    .and_then(|s| s.as_str())
                    .unwrap_or(server)
                    .into(),
                fingerprint: proxy
                    .get("client-fingerprint")
                    .and_then(|f| f.as_str())
                    .unwrap_or("chrome")
                    .into(),
                public_key: reality_opts
                    .get("public-key")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .into(),
                short_id: reality_opts
                    .get("short-id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .into(),
                spider_x: None,
            }
        } else if tls {
            SecurityConfig::Tls {
                sni: proxy
                    .get("servername")
                    .and_then(|s| s.as_str())
                    .unwrap_or(server)
                    .into(),
                fingerprint: proxy
                    .get("client-fingerprint")
                    .and_then(|f| f.as_str())
                    .map(String::from),
                alpn: None,
            }
        } else {
            SecurityConfig::None
        };

        let mut config = ServerConfig::new_vless(
            name.into(),
            server.into(),
            port,
            uuid.into(),
            transport,
            security,
        );

        if let Some(f) = flow {
            if !f.is_empty() {
                config = config.with_flow(f);
            }
        }

        servers.push(config);
    }

    if servers.is_empty() {
        return Err(anyhow!("no VLESS servers found in Clash YAML"));
    }

    Ok(servers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_base64_subscription() {
        let uris = "vless://uuid1@server1.com:443?type=tcp&security=reality&fp=chrome&pbk=key1&sid=01&sni=sni1.com&flow=xtls-rprx-vision&encryption=none#Server-1\n\
                     vless://uuid2@server2.com:443?type=tcp&security=reality&fp=chrome&pbk=key2&sid=02&sni=sni2.com&flow=xtls-rprx-vision&encryption=none#Server-2";
        let encoded = general_purpose::STANDARD.encode(uris);

        let servers = parse_subscription(&encoded).unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "Server-1");
        assert_eq!(servers[1].name, "Server-2");
    }

    #[test]
    fn parse_plain_text_uris() {
        let body = "vless://uuid1@server1.com:443?type=tcp&security=none&encryption=none#S1\n\
                     // This is a comment\n\
                     \n\
                     vless://uuid2@server2.com:443?type=tcp&security=none&encryption=none#S2";

        let servers = parse_subscription(body).unwrap();
        assert_eq!(servers.len(), 2);
    }

    #[test]
    fn detect_singbox_format() {
        let json = r#"{"outbounds": [{"type": "vless", "server": "test.com", "server_port": 443, "uuid": "test"}]}"#;
        assert_eq!(detect_format(json), SubscriptionFormat::SingBoxJson);
    }

    #[test]
    fn detect_clash_format() {
        let yaml = "proxies:\n  - name: test\n    type: vless\n";
        assert_eq!(detect_format(yaml), SubscriptionFormat::ClashYaml);
    }

    #[test]
    fn detect_base64_format() {
        // Neither JSON nor YAML with "proxies:" -- defaults to Base64Uris
        let body = "dmxlc3M6Ly91dWlkQGhvc3Q6NDQz";
        assert_eq!(detect_format(body), SubscriptionFormat::Base64Uris);
    }

    #[test]
    fn detect_plain_text_as_base64() {
        // Plain text that is not JSON or YAML
        let body = "just some random text";
        assert_eq!(detect_format(body), SubscriptionFormat::Base64Uris);
    }

    #[test]
    fn detect_json_array_as_singbox() {
        let json = r#"[{"outbounds": []}]"#;
        assert_eq!(detect_format(json), SubscriptionFormat::SingBoxJson);
    }

    #[test]
    fn detect_clash_with_leading_content() {
        let yaml = "# comment\nproxies:\n  - name: test\n";
        assert_eq!(detect_format(yaml), SubscriptionFormat::ClashYaml);
    }

    #[test]
    fn parse_singbox_json_subscription() {
        let json = r#"{
            "outbounds": [
                {
                    "type": "vless",
                    "tag": "NL-Reality",
                    "server": "nl.example.com",
                    "server_port": 443,
                    "uuid": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
                    "flow": "xtls-rprx-vision",
                    "tls": {
                        "enabled": true,
                        "reality": {
                            "enabled": true,
                            "public_key": "testpublickey123",
                            "short_id": "ab"
                        },
                        "server_name": "www.microsoft.com",
                        "utls": {
                            "fingerprint": "chrome"
                        }
                    }
                },
                {
                    "type": "selector",
                    "tag": "select",
                    "outbounds": ["NL-Reality"]
                }
            ]
        }"#;

        let servers = parse_subscription(json).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "NL-Reality");
        assert_eq!(servers[0].address, "nl.example.com");
        assert_eq!(servers[0].port, 443);
        assert_eq!(servers[0].uuid, "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
        assert_eq!(servers[0].flow.as_deref(), Some("xtls-rprx-vision"));
        match &servers[0].security {
            crate::models::SecurityConfig::Reality { public_key, short_id, sni, .. } => {
                assert_eq!(public_key, "testpublickey123");
                assert_eq!(short_id, "ab");
                assert_eq!(sni, "www.microsoft.com");
            }
            _ => panic!("expected Reality security from SingBox JSON"),
        }
    }

    #[test]
    fn parse_singbox_json_no_vless_returns_error() {
        let json = r#"{
            "outbounds": [
                {"type": "selector", "tag": "select", "outbounds": []}
            ]
        }"#;
        assert!(parse_subscription(json).is_err());
    }

    #[test]
    fn parse_clash_yaml_subscription() {
        let yaml = r#"proxies:
  - name: NL-Server
    type: vless
    server: nl.example.com
    port: 443
    uuid: a1b2c3d4-e5f6-7890-abcd-ef1234567890
    flow: xtls-rprx-vision
    tls: true
    servername: www.microsoft.com
    client-fingerprint: chrome
    reality-opts:
      public-key: testpublickey456
      short-id: cd
"#;

        let servers = parse_subscription(yaml).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "NL-Server");
        assert_eq!(servers[0].address, "nl.example.com");
        assert_eq!(servers[0].port, 443);
        assert_eq!(servers[0].flow.as_deref(), Some("xtls-rprx-vision"));
        match &servers[0].security {
            crate::models::SecurityConfig::Reality { public_key, short_id, sni, .. } => {
                assert_eq!(public_key, "testpublickey456");
                assert_eq!(short_id, "cd");
                assert_eq!(sni, "www.microsoft.com");
            }
            _ => panic!("expected Reality security from Clash YAML"),
        }
    }

    #[test]
    fn parse_clash_yaml_ws_tls() {
        let yaml = r#"proxies:
  - name: WS-Server
    type: vless
    server: ws.example.com
    port: 443
    uuid: a1b2c3d4-e5f6-7890-abcd-ef1234567890
    tls: true
    servername: cdn.example.com
    network: ws
    ws-opts:
      path: /vless-ws
      headers:
        Host: cdn.example.com
"#;

        let servers = parse_subscription(yaml).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "WS-Server");
        match &servers[0].transport {
            crate::models::TransportConfig::Ws { path, host } => {
                assert_eq!(path, "/vless-ws");
                assert_eq!(host.as_deref(), Some("cdn.example.com"));
            }
            _ => panic!("expected WS transport from Clash YAML"),
        }
        assert!(matches!(servers[0].security, crate::models::SecurityConfig::Tls { .. }));
    }

    #[test]
    fn parse_clash_yaml_no_vless_returns_error() {
        let yaml = "proxies:\n  - name: SS\n    type: ss\n    server: ss.com\n    port: 443\n";
        assert!(parse_subscription(yaml).is_err());
    }

    #[test]
    fn parse_base64_skips_comments_and_blanks() {
        let uris = "vless://uuid1@server1.com:443?type=tcp&security=none&encryption=none#S1\n\
                     // skip this\n\
                     # also skip\n\
                     \n\
                     vless://uuid2@server2.com:443?type=tcp&security=none&encryption=none#S2";
        let encoded = general_purpose::STANDARD.encode(uris);
        let servers = parse_subscription(&encoded).unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "S1");
        assert_eq!(servers[1].name, "S2");
    }

    #[test]
    fn parse_base64_url_safe_encoding() {
        let uris = "vless://uuid1@server1.com:443?type=tcp&security=none&encryption=none#S1";
        let encoded = general_purpose::URL_SAFE.encode(uris);
        let servers = parse_subscription(&encoded).unwrap();
        assert_eq!(servers.len(), 1);
    }

    #[test]
    fn parse_base64_no_valid_servers_returns_error() {
        let uris = "vmess://something\nss://something_else";
        let encoded = general_purpose::STANDARD.encode(uris);
        assert!(parse_subscription(&encoded).is_err());
    }

    #[test]
    fn parse_singbox_tls_without_reality() {
        let json = r#"{
            "outbounds": [
                {
                    "type": "vless",
                    "tag": "TLS-Only",
                    "server": "tls.example.com",
                    "server_port": 443,
                    "uuid": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
                    "tls": {
                        "enabled": true,
                        "server_name": "tls.example.com"
                    },
                    "transport": {
                        "type": "ws",
                        "path": "/ws-path",
                        "headers": {
                            "Host": "cdn.example.com"
                        }
                    }
                }
            ]
        }"#;

        let servers = parse_subscription(json).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "TLS-Only");
        match &servers[0].security {
            crate::models::SecurityConfig::Tls { sni, .. } => {
                assert_eq!(sni, "tls.example.com");
            }
            _ => panic!("expected TLS security"),
        }
        match &servers[0].transport {
            crate::models::TransportConfig::Ws { path, host } => {
                assert_eq!(path, "/ws-path");
                assert_eq!(host.as_deref(), Some("cdn.example.com"));
            }
            _ => panic!("expected WS transport"),
        }
    }
}

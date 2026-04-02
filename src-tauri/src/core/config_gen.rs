use anyhow::Result;
use serde_json::json;

use crate::models::{SecurityConfig, ServerConfig, Settings, TransportConfig};

pub struct ConfigGenerator;

impl ConfigGenerator {
    /// Generate xray-core JSON config with SOCKS5 + HTTP inbounds.
    pub fn generate_xray_config(server: &ServerConfig, settings: &Settings) -> Result<serde_json::Value> {
        let mut user = json!({
            "id": server.uuid,
            "encryption": server.encryption,
        });
        if let Some(flow) = &server.flow {
            user["flow"] = json!(flow);
        }

        let mut stream_settings = json!({});

        // Network / transport
        match &server.transport {
            TransportConfig::Tcp => {
                stream_settings["network"] = json!("tcp");
            }
            TransportConfig::Ws { path, host } => {
                stream_settings["network"] = json!("ws");
                let mut ws = json!({ "path": path });
                if let Some(h) = host {
                    ws["headers"] = json!({ "Host": h });
                }
                stream_settings["wsSettings"] = ws;
            }
            TransportConfig::Grpc { service_name } => {
                stream_settings["network"] = json!("grpc");
                stream_settings["grpcSettings"] = json!({
                    "serviceName": service_name,
                });
            }
            TransportConfig::Xhttp { path, host, mode } => {
                stream_settings["network"] = json!("xhttp");
                let mut xhttp = json!({ "path": path });
                if let Some(h) = host {
                    xhttp["host"] = json!(h);
                }
                if let Some(m) = mode {
                    xhttp["mode"] = json!(m);
                }
                stream_settings["xhttpSettings"] = xhttp;
            }
        }

        // Security
        match &server.security {
            SecurityConfig::None => {
                stream_settings["security"] = json!("none");
            }
            SecurityConfig::Tls {
                sni,
                fingerprint,
                alpn,
            } => {
                stream_settings["security"] = json!("tls");
                let mut tls = json!({ "serverName": sni });
                // Default to chrome fingerprint for anti-detection
                tls["fingerprint"] = json!(fingerprint.as_deref().unwrap_or("chrome"));
                if let Some(alpn) = alpn {
                    tls["alpn"] = json!(alpn);
                }
                stream_settings["tlsSettings"] = tls;
            }
            SecurityConfig::Reality {
                sni,
                fingerprint,
                public_key,
                short_id,
                spider_x,
            } => {
                stream_settings["security"] = json!("reality");
                let reality = json!({
                    "show": false,
                    "fingerprint": fingerprint,
                    "serverName": sni,
                    "publicKey": public_key,
                    "shortId": short_id,
                    "spiderX": spider_x.as_deref().unwrap_or(""),
                });
                stream_settings["realitySettings"] = reality;
            }
        }

        let config = json!({
            "log": {
                "loglevel": "info",
            },
            "stats": {},
            "api": {
                "tag": "api",
                "services": ["StatsService"],
            },
            "inbounds": [
                {
                    "tag": "socks-in",
                    "port": settings.xray_socks_port,
                    "listen": "127.0.0.1",
                    "protocol": "socks",
                    "settings": {
                        "auth": "noauth",
                        "udp": true,
                        "ip": "127.0.0.1",
                    },
                    "sniffing": {
                        "enabled": true,
                        "destOverride": ["http", "tls", "quic"],
                        "routeOnly": true,
                    },
                },
                {
                    "tag": "http-in",
                    "port": settings.xray_http_port,
                    "listen": "127.0.0.1",
                    "protocol": "http",
                    "settings": {
                        "allowTransparent": false,
                    },
                    "sniffing": {
                        "enabled": true,
                        "destOverride": ["http", "tls"],
                        "routeOnly": true,
                    },
                },
                {
                    "tag": "api-in",
                    "port": settings.xray_api_port,
                    "listen": "127.0.0.1",
                    "protocol": "dokodemo-door",
                    "settings": {
                        "address": "127.0.0.1",
                    },
                },
            ],
            "outbounds": [
                {
                    "tag": "proxy",
                    "protocol": "vless",
                    "settings": {
                        "vnext": [{
                            "address": server.address,
                            "port": server.port,
                            "users": [user],
                        }],
                    },
                    "streamSettings": stream_settings,
                },
                {
                    "tag": "direct",
                    "protocol": "freedom",
                    "settings": {
                        "domainStrategy": "AsIs",
                    },
                },
                {
                    "tag": "block",
                    "protocol": "blackhole",
                    "settings": {},
                },
            ],
            "routing": {
                "domainStrategy": "AsIs",
                "rules": [
                    {
                        "type": "field",
                        "inboundTag": ["api-in"],
                        "outboundTag": "api",
                    },
                    {
                        "type": "field",
                        "ip": ["geoip:private"],
                        "outboundTag": "direct",
                    },
                    // Bypass Windows system traffic: updates, certificates, telemetry
                    {
                        "type": "field",
                        "domain": [
                            "domain:windowsupdate.com",
                            "domain:microsoft.com",
                            "domain:msftconnecttest.com",
                            "domain:msftncsi.com",
                            "domain:pki.goog",
                            "domain:lencr.org",
                            "domain:digicert.com",
                        ],
                        "outboundTag": "direct",
                    },
                ],
            },
        });

        Ok(config)
    }
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
    fn generate_xray_config_reality() {
        let server = test_server();
        let settings = Settings::default();
        let config = ConfigGenerator::generate_xray_config(&server, &settings).unwrap();

        let outbound = &config["outbounds"][0];
        assert_eq!(outbound["protocol"], "vless");
        assert_eq!(outbound["streamSettings"]["security"], "reality");
        assert_eq!(
            outbound["settings"]["vnext"][0]["users"][0]["flow"],
            "xtls-rprx-vision"
        );
        // SOCKS5 inbound
        assert_eq!(config["inbounds"][0]["port"], 10808);
        assert_eq!(config["inbounds"][0]["protocol"], "socks");
        // HTTP inbound
        assert_eq!(config["inbounds"][1]["port"], 10809);
        assert_eq!(config["inbounds"][1]["protocol"], "http");
    }

    #[test]
    fn generate_xray_config_tls_default_fingerprint() {
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
        let config = ConfigGenerator::generate_xray_config(&server, &settings).unwrap();

        // Should default to chrome fingerprint
        assert_eq!(
            config["outbounds"][0]["streamSettings"]["tlsSettings"]["fingerprint"],
            "chrome"
        );
    }

    #[test]
    fn generate_xray_config_ws_transport() {
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
        let config = ConfigGenerator::generate_xray_config(&server, &settings).unwrap();

        assert_eq!(config["outbounds"][0]["streamSettings"]["network"], "ws");
        assert_eq!(
            config["outbounds"][0]["streamSettings"]["wsSettings"]["path"],
            "/ws"
        );
    }

    #[test]
    fn generate_xray_config_has_stats_api() {
        let server = test_server();
        let settings = Settings::default();
        let config = ConfigGenerator::generate_xray_config(&server, &settings).unwrap();

        assert!(config.get("stats").is_some());
        assert_eq!(config["api"]["services"][0], "StatsService");
    }
}

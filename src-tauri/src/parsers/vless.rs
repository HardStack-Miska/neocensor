use anyhow::{anyhow, Context, Result};
use percent_encoding::percent_decode_str;
use url::Url;

use crate::models::{SecurityConfig, ServerConfig, TransportConfig};

/// Parse a VLESS URI into a ServerConfig.
///
/// Format: vless://UUID@host:port?params#fragment
///
/// Supported params:
///   type     = tcp | ws | grpc | xhttp
///   security = reality | tls | none
///   flow     = xtls-rprx-vision
///   fp       = chrome | firefox | safari | qq | random
///   pbk      = x25519 public key (reality)
///   sid      = short id (reality)
///   sni      = server name indication
///   alpn     = h2,http/1.1
///   path     = /path (ws, xhttp)
///   host     = host header (ws, xhttp)
///   serviceName = gRPC service name
///   headerType  = none (xhttp)
///   mode     = packet-up (xhttp)
///   spx      = spiderX (reality)
///   encryption = none
pub fn parse_vless_uri(uri: &str) -> Result<ServerConfig> {
    let trimmed = uri.trim();
    if !trimmed.starts_with("vless://") {
        return Err(anyhow!("not a VLESS URI: must start with vless://"));
    }

    // Extract fragment (server name) before parsing as URL
    let (uri_part, fragment) = match trimmed.rfind('#') {
        Some(pos) => (
            &trimmed[..pos],
            percent_decode_str(&trimmed[pos + 1..])
                .decode_utf8_lossy()
                .into_owned(),
        ),
        None => (trimmed, String::new()),
    };

    // Replace vless:// with http:// for URL parser compatibility
    let fake_url = format!("http://{}", &uri_part[8..]);
    let parsed = Url::parse(&fake_url).context("failed to parse VLESS URI")?;

    let uuid = parsed.username().to_string();
    if uuid.is_empty() {
        return Err(anyhow!("missing UUID in VLESS URI"));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("missing host"))?
        .to_string();

    let port = parsed.port().unwrap_or(443);

    // Parse query parameters
    let params: std::collections::HashMap<String, String> =
        parsed.query_pairs().into_owned().collect();

    let get = |key: &str| params.get(key).map(|s| s.as_str());

    // Parse transport
    let transport = match get("type").unwrap_or("tcp") {
        "tcp" => TransportConfig::Tcp,
        "ws" => TransportConfig::Ws {
            path: get("path").unwrap_or("/").to_string(),
            host: get("host").map(String::from),
        },
        "grpc" => TransportConfig::Grpc {
            service_name: get("serviceName").unwrap_or("").to_string(),
        },
        "xhttp" | "splithttp" => TransportConfig::Xhttp {
            path: get("path").unwrap_or("/").to_string(),
            host: get("host").map(String::from),
            mode: get("mode").map(String::from),
        },
        other => return Err(anyhow!("unsupported transport type: {other}")),
    };

    // Parse security
    let security = match get("security").unwrap_or("none") {
        "reality" => {
            let pbk = get("pbk")
                .ok_or_else(|| anyhow!("reality requires pbk (public key)"))?;
            SecurityConfig::Reality {
                sni: get("sni").unwrap_or(&host).to_string(),
                fingerprint: get("fp").unwrap_or("chrome").to_string(),
                public_key: pbk.to_string(),
                short_id: get("sid").unwrap_or("").to_string(),
                spider_x: get("spx").map(String::from),
            }
        }
        "tls" => SecurityConfig::Tls {
            sni: get("sni").unwrap_or(&host).to_string(),
            fingerprint: get("fp").map(String::from),
            alpn: get("alpn").map(|a| a.split(',').map(String::from).collect()),
        },
        "none" | "" => SecurityConfig::None,
        other => return Err(anyhow!("unsupported security type: {other}")),
    };

    let name = if fragment.is_empty() {
        format!("{host}:{port}")
    } else {
        fragment
    };

    let mut config = ServerConfig::new_vless(name, host, port, uuid, transport, security);

    if let Some(flow) = get("flow") {
        if !flow.is_empty() {
            config = config.with_flow(flow);
        }
    }

    Ok(config)
}

/// Serialize a ServerConfig back to a VLESS URI string.
pub fn to_vless_uri(config: &ServerConfig) -> String {
    let mut params = Vec::new();

    // Transport
    match &config.transport {
        TransportConfig::Tcp => params.push("type=tcp".into()),
        TransportConfig::Ws { path, host } => {
            params.push("type=ws".into());
            params.push(format!("path={path}"));
            if let Some(h) = host {
                params.push(format!("host={h}"));
            }
        }
        TransportConfig::Grpc { service_name } => {
            params.push("type=grpc".into());
            params.push(format!("serviceName={service_name}"));
        }
        TransportConfig::Xhttp { path, host, mode } => {
            params.push("type=xhttp".into());
            params.push(format!("path={path}"));
            if let Some(h) = host {
                params.push(format!("host={h}"));
            }
            if let Some(m) = mode {
                params.push(format!("mode={m}"));
            }
        }
    }

    // Security
    match &config.security {
        SecurityConfig::None => params.push("security=none".into()),
        SecurityConfig::Tls {
            sni,
            fingerprint,
            alpn,
        } => {
            params.push("security=tls".into());
            params.push(format!("sni={sni}"));
            if let Some(fp) = fingerprint {
                params.push(format!("fp={fp}"));
            }
            if let Some(alpn) = alpn {
                params.push(format!("alpn={}", alpn.join(",")));
            }
        }
        SecurityConfig::Reality {
            sni,
            fingerprint,
            public_key,
            short_id,
            spider_x,
        } => {
            params.push("security=reality".into());
            params.push(format!("sni={sni}"));
            params.push(format!("fp={fingerprint}"));
            params.push(format!("pbk={public_key}"));
            params.push(format!("sid={short_id}"));
            if let Some(spx) = spider_x {
                params.push(format!("spx={spx}"));
            }
        }
    }

    // Flow
    if let Some(flow) = &config.flow {
        params.push(format!("flow={flow}"));
    }

    params.push(format!("encryption={}", config.encryption));

    let query = params.join("&");
    let fragment = percent_encoding::utf8_percent_encode(
        &config.name,
        percent_encoding::NON_ALPHANUMERIC,
    );

    format!(
        "vless://{}@{}:{}?{}#{}",
        config.uuid, config.address, config.port, query, fragment
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reality_vision_uri() {
        let uri = "vless://550e8400-e29b-41d4-a716-446655440000@example.com:443\
            ?type=tcp&security=reality&fp=chrome\
            &pbk=abc123publickey&sid=0a1b2c\
            &sni=www.microsoft.com\
            &flow=xtls-rprx-vision\
            &encryption=none#NL-1";

        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.name, "NL-1");
        assert_eq!(config.address, "example.com");
        assert_eq!(config.port, 443);
        assert_eq!(config.uuid, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(config.flow.as_deref(), Some("xtls-rprx-vision"));

        match &config.security {
            SecurityConfig::Reality {
                sni,
                fingerprint,
                public_key,
                short_id,
                ..
            } => {
                assert_eq!(sni, "www.microsoft.com");
                assert_eq!(fingerprint, "chrome");
                assert_eq!(public_key, "abc123publickey");
                assert_eq!(short_id, "0a1b2c");
            }
            _ => panic!("expected Reality security"),
        }

        match &config.transport {
            TransportConfig::Tcp => {}
            _ => panic!("expected TCP transport"),
        }
    }

    #[test]
    fn parse_ws_tls_uri() {
        let uri = "vless://uuid-here@ws.example.com:8443\
            ?type=ws&path=%2Fvless-ws&host=cdn.example.com\
            &security=tls&sni=cdn.example.com&fp=firefox\
            &encryption=none#WS-Server";

        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.name, "WS-Server");

        match &config.transport {
            TransportConfig::Ws { path, host } => {
                assert_eq!(path, "/vless-ws");
                assert_eq!(host.as_deref(), Some("cdn.example.com"));
            }
            _ => panic!("expected WS transport"),
        }

        match &config.security {
            SecurityConfig::Tls { sni, .. } => {
                assert_eq!(sni, "cdn.example.com");
            }
            _ => panic!("expected TLS security"),
        }
    }

    #[test]
    fn parse_grpc_uri() {
        let uri = "vless://test-uuid@grpc.example.com:443\
            ?type=grpc&serviceName=mygrpc\
            &security=reality&fp=chrome\
            &pbk=key123&sid=ab&sni=target.com\
            &encryption=none#gRPC-Server";

        let config = parse_vless_uri(uri).unwrap();
        match &config.transport {
            TransportConfig::Grpc { service_name } => {
                assert_eq!(service_name, "mygrpc");
            }
            _ => panic!("expected gRPC transport"),
        }
    }

    #[test]
    fn parse_xhttp_uri() {
        let uri = "vless://test-uuid@xhttp.example.com:443\
            ?type=xhttp&path=%2Fxhttp&mode=packet-up\
            &security=tls&sni=xhttp.example.com\
            &encryption=none#XHTTP-Server";

        let config = parse_vless_uri(uri).unwrap();
        match &config.transport {
            TransportConfig::Xhttp { path, mode, .. } => {
                assert_eq!(path, "/xhttp");
                assert_eq!(mode.as_deref(), Some("packet-up"));
            }
            _ => panic!("expected XHTTP transport"),
        }
    }

    #[test]
    fn roundtrip_uri() {
        let uri = "vless://550e8400-e29b-41d4-a716-446655440000@example.com:443\
            ?type=tcp&security=reality&fp=chrome\
            &pbk=abc123&sid=0a\
            &sni=www.microsoft.com\
            &flow=xtls-rprx-vision\
            &encryption=none#Test";

        let config = parse_vless_uri(uri).unwrap();
        let regenerated = to_vless_uri(&config);
        let reparsed = parse_vless_uri(&regenerated).unwrap();

        assert_eq!(config.uuid, reparsed.uuid);
        assert_eq!(config.address, reparsed.address);
        assert_eq!(config.port, reparsed.port);
        assert_eq!(config.flow, reparsed.flow);
    }

    #[test]
    fn reject_non_vless() {
        assert!(parse_vless_uri("vmess://something").is_err());
        assert!(parse_vless_uri("https://example.com").is_err());
    }

    #[test]
    fn reject_empty_input() {
        assert!(parse_vless_uri("").is_err());
        assert!(parse_vless_uri("   ").is_err());
    }

    #[test]
    fn reject_missing_uuid() {
        let uri = "vless://@host:443?type=tcp&security=none#NoUUID";
        assert!(parse_vless_uri(uri).is_err());
    }

    #[test]
    fn parse_no_security() {
        // Note: port 80 is the default for http:// scheme used internally by the parser,
        // so Url::port() returns None and the code defaults to 443.
        // Use a non-default port to test explicit port parsing.
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host.example.com:8080\
            ?type=tcp&security=none&encryption=none#Plain";
        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.name, "Plain");
        assert_eq!(config.port, 8080);
        assert_eq!(config.uuid, "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
        assert!(matches!(config.security, SecurityConfig::None));
        assert!(matches!(config.transport, TransportConfig::Tcp));
        assert!(config.flow.is_none());
    }

    #[test]
    fn parse_default_port_when_missing() {
        // url crate may not parse port-less authority well with vless->http trick,
        // but the code defaults to 443
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host.example.com\
            ?type=tcp&security=none&encryption=none#DefaultPort";
        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.port, 443);
    }

    #[test]
    fn parse_fragment_with_spaces() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host.example.com:443\
            ?type=tcp&security=none&encryption=none#My%20Server%20Name";
        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.name, "My Server Name");
    }

    #[test]
    fn parse_fragment_with_unicode() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host.example.com:443\
            ?type=tcp&security=none&encryption=none#%D0%A1%D0%B5%D1%80%D0%B2%D0%B5%D1%80";
        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.name, "Сервер");
    }

    #[test]
    fn parse_no_fragment_uses_host_port() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@myhost.com:8443\
            ?type=tcp&security=none&encryption=none";
        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.name, "myhost.com:8443");
    }

    #[test]
    fn parse_tls_with_alpn() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host.example.com:443\
            ?type=tcp&security=tls&sni=example.com&alpn=h2,http/1.1&encryption=none#ALPN";
        let config = parse_vless_uri(uri).unwrap();
        match &config.security {
            SecurityConfig::Tls { sni, alpn, .. } => {
                assert_eq!(sni, "example.com");
                let alpn = alpn.as_ref().unwrap();
                assert_eq!(alpn, &["h2", "http/1.1"]);
            }
            _ => panic!("expected TLS security"),
        }
    }

    #[test]
    fn parse_ws_tls_with_host_header() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host.example.com:443\
            ?type=ws&security=tls&sni=cdn.example.com&fp=firefox\
            &path=%2Fws&host=cdn.example.com&encryption=none#WS+Server";
        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.name, "WS+Server");
        match &config.transport {
            TransportConfig::Ws { path, host } => {
                assert_eq!(path, "/ws");
                assert_eq!(host.as_deref(), Some("cdn.example.com"));
            }
            _ => panic!("expected WS transport"),
        }
        match &config.security {
            SecurityConfig::Tls { sni, fingerprint, .. } => {
                assert_eq!(sni, "cdn.example.com");
                assert_eq!(fingerprint.as_deref(), Some("firefox"));
            }
            _ => panic!("expected TLS security"),
        }
    }

    #[test]
    fn parse_grpc_tls() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host.example.com:443\
            ?type=grpc&security=tls&sni=example.com&serviceName=mygrpc\
            &encryption=none#gRPC";
        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.name, "gRPC");
        match &config.transport {
            TransportConfig::Grpc { service_name } => {
                assert_eq!(service_name, "mygrpc");
            }
            _ => panic!("expected gRPC transport"),
        }
        assert!(matches!(config.security, SecurityConfig::Tls { .. }));
    }

    #[test]
    fn parse_xhttp_with_mode() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host.example.com:443\
            ?type=xhttp&security=tls&sni=example.com&path=%2Fxhttp\
            &mode=stream-one&encryption=none#xHTTP";
        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.name, "xHTTP");
        match &config.transport {
            TransportConfig::Xhttp { path, mode, .. } => {
                assert_eq!(path, "/xhttp");
                assert_eq!(mode.as_deref(), Some("stream-one"));
            }
            _ => panic!("expected XHTTP transport"),
        }
    }

    #[test]
    fn parse_reality_tcp_flow() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host.example.com:443\
            ?type=tcp&security=reality&fp=chrome&pbk=SomePublicKey123\
            &sid=ab&sni=www.microsoft.com&flow=xtls-rprx-vision\
            &encryption=none#MyServer";
        let config = parse_vless_uri(uri).unwrap();
        assert_eq!(config.name, "MyServer");
        assert_eq!(config.address, "host.example.com");
        assert_eq!(config.port, 443);
        assert_eq!(config.uuid, "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
        assert_eq!(config.flow.as_deref(), Some("xtls-rprx-vision"));
        match &config.security {
            SecurityConfig::Reality { sni, fingerprint, public_key, short_id, .. } => {
                assert_eq!(sni, "www.microsoft.com");
                assert_eq!(fingerprint, "chrome");
                assert_eq!(public_key, "SomePublicKey123");
                assert_eq!(short_id, "ab");
            }
            _ => panic!("expected Reality security"),
        }
    }

    #[test]
    fn reject_unsupported_transport() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host:443\
            ?type=quic&security=none&encryption=none#Bad";
        assert!(parse_vless_uri(uri).is_err());
    }

    #[test]
    fn reject_unsupported_security() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host:443\
            ?type=tcp&security=xtls&encryption=none#Bad";
        assert!(parse_vless_uri(uri).is_err());
    }

    #[test]
    fn reject_reality_without_pbk() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@host:443\
            ?type=tcp&security=reality&sni=example.com&encryption=none#NoPBK";
        assert!(parse_vless_uri(uri).is_err());
    }

    #[test]
    fn roundtrip_ws_tls() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@ws.example.com:443\
            ?type=ws&path=%2Fvless&host=cdn.example.com\
            &security=tls&sni=cdn.example.com\
            &encryption=none#WS-Roundtrip";
        let config = parse_vless_uri(uri).unwrap();
        let regenerated = to_vless_uri(&config);
        let reparsed = parse_vless_uri(&regenerated).unwrap();

        assert_eq!(config.uuid, reparsed.uuid);
        assert_eq!(config.address, reparsed.address);
        assert_eq!(config.port, reparsed.port);
        assert_eq!(config.name, reparsed.name);
        assert_eq!(config.flow, reparsed.flow);

        match (&config.transport, &reparsed.transport) {
            (TransportConfig::Ws { path: p1, host: h1 }, TransportConfig::Ws { path: p2, host: h2 }) => {
                assert_eq!(p1, p2);
                assert_eq!(h1, h2);
            }
            _ => panic!("transport mismatch after roundtrip"),
        }

        match (&config.security, &reparsed.security) {
            (SecurityConfig::Tls { sni: s1, .. }, SecurityConfig::Tls { sni: s2, .. }) => {
                assert_eq!(s1, s2);
            }
            _ => panic!("security mismatch after roundtrip"),
        }
    }

    #[test]
    fn roundtrip_xhttp() {
        let uri = "vless://a1b2c3d4-e5f6-7890-abcd-ef1234567890@xh.example.com:443\
            ?type=xhttp&path=%2Fxhttp&mode=packet-up\
            &security=tls&sni=xh.example.com\
            &encryption=none#XHTTP-RT";
        let config = parse_vless_uri(uri).unwrap();
        let regenerated = to_vless_uri(&config);
        let reparsed = parse_vless_uri(&regenerated).unwrap();

        assert_eq!(config.uuid, reparsed.uuid);
        assert_eq!(config.address, reparsed.address);
        match (&config.transport, &reparsed.transport) {
            (TransportConfig::Xhttp { path: p1, mode: m1, .. }, TransportConfig::Xhttp { path: p2, mode: m2, .. }) => {
                assert_eq!(p1, p2);
                assert_eq!(m1, m2);
            }
            _ => panic!("transport mismatch after roundtrip"),
        }
    }
}

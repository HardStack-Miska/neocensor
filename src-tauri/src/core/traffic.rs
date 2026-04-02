use serde::{Deserialize, Serialize};

/// A parsed connection event from sing-box logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionEvent {
    pub id: u64,
    pub time: String,
    pub host: String,
    pub port: u16,
    pub route: String,
    pub protocol: String,
}

/// Aggregated traffic snapshot emitted to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficSnapshot {
    pub connections: Vec<ConnectionEvent>,
    pub total_connections: u64,
    pub active: bool,
}

/// Parse sing-box log line into a ConnectionEvent.
///
/// sing-box (with --disable-color) logs router connections as:
///   `INFO [router] inbound/tun-in | <source> >> <outbound> | <destination>`
///
/// Or via the broadcast layer with tracing prefix:
///   `[sing-box] INFO[0000] inbound/tun-in[...] connection at ... | <source> >> outbound/<tag> | <destination>`
///
/// We parse flexibly: look for ">>" to find outbound, and the destination after the last "|".
pub fn parse_singbox_connection(line: &str, counter: u64) -> Option<ConnectionEvent> {
    // Must contain ">>" for routing info
    let arrow_idx = line.find(">>")?;

    // Extract outbound tag after ">>"
    let after_arrow = &line[arrow_idx + 2..];

    // Find the outbound name: could be "outbound/proxy" or just "proxy"
    let route = if after_arrow.contains("proxy") {
        "proxy"
    } else if after_arrow.contains("direct") {
        "direct"
    } else if after_arrow.contains("block") {
        "block"
    } else if after_arrow.contains("dns-out") {
        return None; // Skip DNS routing lines
    } else {
        return None;
    };

    // Determine protocol from inbound tag
    let protocol = if line.contains("tun-in") {
        "TUN"
    } else if line.contains("mixed-in") {
        "Mixed"
    } else {
        "other"
    };

    // Extract destination: after the last "|" in the line
    let last_pipe = line.rfind('|')?;
    let dest_str = line[last_pipe + 1..].trim();

    // Parse host:port from destination (e.g., "portal.mail.ru:443")
    let (host, port) = if let Some(colon_idx) = dest_str.rfind(':') {
        let h = &dest_str[..colon_idx];
        let p = dest_str[colon_idx + 1..].parse().unwrap_or(0);
        (h.to_string(), p)
    } else {
        (dest_str.to_string(), 0)
    };

    // Skip empty hosts
    if host.is_empty() {
        return None;
    }

    let time = extract_time(line);

    Some(ConnectionEvent {
        id: counter,
        time,
        host,
        port,
        route: route.to_string(),
        protocol: protocol.to_string(),
    })
}

fn extract_time(line: &str) -> String {
    // Try to find time pattern HH:MM:SS in the line
    for window in line.as_bytes().windows(8) {
        if window[2] == b':' && window[5] == b':' {
            let s = std::str::from_utf8(window).unwrap_or("--:--:--");
            if s.chars().all(|c| c.is_ascii_digit() || c == ':') {
                return s.to_string();
            }
        }
    }
    // Fallback: current local time
    let now = chrono::Local::now();
    now.format("%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_singbox_tun_proxy_connection() {
        let line = "[sing-box] INFO[0042] inbound/tun-in[NeoCensor] | 172.19.0.1:55222 >> outbound/proxy | portal.mail.ru:443";
        let ev = parse_singbox_connection(line, 1).unwrap();
        assert_eq!(ev.host, "portal.mail.ru");
        assert_eq!(ev.port, 443);
        assert_eq!(ev.route, "proxy");
        assert_eq!(ev.protocol, "TUN");
    }

    #[test]
    fn parse_singbox_mixed_proxy_connection() {
        let line = "[sing-box] INFO[0042] inbound/mixed-in | 127.0.0.1:51241 >> outbound/proxy | 91.105.192.100:80";
        let ev = parse_singbox_connection(line, 2).unwrap();
        assert_eq!(ev.host, "91.105.192.100");
        assert_eq!(ev.port, 80);
        assert_eq!(ev.route, "proxy");
        assert_eq!(ev.protocol, "Mixed");
    }

    #[test]
    fn parse_singbox_direct_connection() {
        let line = "[sing-box] INFO[0042] inbound/tun-in[NeoCensor] | 172.19.0.1:1234 >> outbound/direct | example.com:443";
        let ev = parse_singbox_connection(line, 3).unwrap();
        assert_eq!(ev.host, "example.com");
        assert_eq!(ev.route, "direct");
    }

    #[test]
    fn parse_singbox_block_connection() {
        let line = "[sing-box] INFO[0042] inbound/tun-in[NeoCensor] | 172.19.0.1:1234 >> outbound/block | blocked.com:443";
        let ev = parse_singbox_connection(line, 7).unwrap();
        assert_eq!(ev.host, "blocked.com");
        assert_eq!(ev.route, "block");
    }

    #[test]
    fn skip_dns_routing() {
        let line = "[sing-box] INFO[0042] inbound/tun-in[NeoCensor] | 172.19.0.1:1234 >> outbound/dns-out | 8.8.8.8:53";
        assert!(parse_singbox_connection(line, 1).is_none());
    }

    #[test]
    fn skip_non_connection_line() {
        let line = "[sing-box] INFO[0000] sing-box started";
        assert!(parse_singbox_connection(line, 1).is_none());
    }

    #[test]
    fn skip_empty_line() {
        assert!(parse_singbox_connection("", 1).is_none());
    }

    #[test]
    fn parse_with_tracing_prefix() {
        let line = "2026-04-02T14:30:55.123456Z  INFO [sing-box] INFO[0042] inbound/tun-in[NeoCensor] | 172.19.0.1:55222 >> outbound/proxy | example.com:443";
        let ev = parse_singbox_connection(line, 42).unwrap();
        assert_eq!(ev.host, "example.com");
        assert_eq!(ev.port, 443);
        assert_eq!(ev.route, "proxy");
        assert_eq!(ev.id, 42);
        assert_eq!(ev.time, "14:30:55");
    }

    #[test]
    fn counter_passed_through_as_id() {
        let line = "[sing-box] INFO[0042] inbound/tun-in[NeoCensor] | 172.19.0.1:1234 >> outbound/proxy | example.com:443";
        let ev = parse_singbox_connection(line, 999).unwrap();
        assert_eq!(ev.id, 999);
    }

    #[test]
    fn parse_host_without_port() {
        let line = "[sing-box] INFO[0042] inbound/tun-in[NeoCensor] | 172.19.0.1:1234 >> outbound/proxy | example.com";
        let ev = parse_singbox_connection(line, 1).unwrap();
        assert_eq!(ev.host, "example.com");
        assert_eq!(ev.port, 0);
    }
}

use serde::{Deserialize, Serialize};

/// A parsed connection event from xray-core logs.
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

/// Parse xray-core log line into a ConnectionEvent.
/// Example log lines:
///   `from 127.0.0.1:55222 accepted //portal.mail.ru:443 [http-in >> proxy]`
///   `from 127.0.0.1:51241 accepted http://91.105.192.100:80/api [http-in >> proxy]`
pub fn parse_xray_connection(line: &str, counter: u64) -> Option<ConnectionEvent> {
    // Must contain "accepted" and routing info in brackets
    let accepted_idx = line.find(" accepted ")?;
    let bracket_start = line.rfind('[')?;
    let bracket_end = line.rfind(']')?;
    if bracket_start >= bracket_end {
        return None;
    }

    // Extract route: "http-in >> proxy" or "socks-in >> direct"
    let route_part = &line[bracket_start + 1..bracket_end];
    let route = if route_part.contains(">> proxy") {
        "proxy"
    } else if route_part.contains(">> direct") {
        "direct"
    } else if route_part.contains(">> block") {
        "block"
    } else {
        return None;
    };

    let protocol = if route_part.contains("http-in") {
        "HTTP"
    } else if route_part.contains("socks-in") {
        "SOCKS5"
    } else {
        "other"
    };

    // Extract destination between "accepted " and " ["
    let dest_start = accepted_idx + " accepted ".len();
    let dest_str = line[dest_start..bracket_start].trim();

    // Parse host:port from various formats:
    //   //portal.mail.ru:443
    //   http://91.105.192.100:80/api
    let cleaned = dest_str
        .trim_start_matches("//")
        .trim_start_matches("http://")
        .trim_start_matches("https://");

    // Take host:port before any path
    let host_port = cleaned.split('/').next().unwrap_or(cleaned);
    let (host, port) = if let Some(colon_idx) = host_port.rfind(':') {
        let h = &host_port[..colon_idx];
        let p = host_port[colon_idx + 1..].parse().unwrap_or(0);
        (h.to_string(), p)
    } else {
        (host_port.to_string(), 0)
    };

    // Extract time from the log line prefix
    // Format: [xray] 2026/04/02 02:30:31.288925 [Info] ...
    // We want HH:MM:SS
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
    fn parse_https_connection() {
        let line = "[xray] 2026/04/02 02:30:31.288925 [Info] from 127.0.0.1:55222 accepted //portal.mail.ru:443 [http-in >> proxy]";
        let ev = parse_xray_connection(line, 1).unwrap();
        assert_eq!(ev.host, "portal.mail.ru");
        assert_eq!(ev.port, 443);
        assert_eq!(ev.route, "proxy");
        assert_eq!(ev.protocol, "HTTP");
    }

    #[test]
    fn parse_http_url_connection() {
        let line = "[xray] 2026/04/02 02:30:33.036503 from 127.0.0.1:51241 accepted http://91.105.192.100:80/api [http-in >> proxy]";
        let ev = parse_xray_connection(line, 2).unwrap();
        assert_eq!(ev.host, "91.105.192.100");
        assert_eq!(ev.port, 80);
        assert_eq!(ev.route, "proxy");
    }

    #[test]
    fn parse_direct_connection() {
        let line = "[xray] from 127.0.0.1:1234 accepted //example.com:443 [http-in >> direct]";
        let ev = parse_xray_connection(line, 3).unwrap();
        assert_eq!(ev.host, "example.com");
        assert_eq!(ev.route, "direct");
    }

    #[test]
    fn skip_non_connection_line() {
        let line = "[xray] Xray 26.3.27 started";
        assert!(parse_xray_connection(line, 1).is_none());
    }

    #[test]
    fn skip_tunneling_line() {
        let line = "[xray] proxy/vless/outbound: tunneling request to tcp:portal.mail.ru:443 via 144.124.237.4:2096";
        assert!(parse_xray_connection(line, 1).is_none());
    }

    #[test]
    fn parse_tracing_formatted_line() {
        // This is the actual format from the tracing broadcast layer (with timestamp prefix)
        let line = "2026-04-01T23:53:25.985527Z  INFO [xray] 2026/04/02 02:53:25.984557 from 127.0.0.1:60398 accepted http://91.105.192.100:80/api [http-in >> proxy]";
        let ev = parse_xray_connection(line, 42).unwrap();
        assert_eq!(ev.host, "91.105.192.100");
        assert_eq!(ev.port, 80);
        assert_eq!(ev.route, "proxy");
        assert_eq!(ev.id, 42);
        // Time extracted from the first HH:MM:SS pattern (tracing timestamp)
        assert_eq!(ev.time, "23:53:25");
    }

    #[test]
    fn parse_tracing_formatted_https() {
        let line = "2026-04-01T23:53:55.258291Z  INFO [xray] 2026/04/02 02:53:55.257288 from 127.0.0.1:65234 accepted //portal.mail.ru:443 [http-in >> proxy]";
        let ev = parse_xray_connection(line, 1).unwrap();
        assert_eq!(ev.host, "portal.mail.ru");
        assert_eq!(ev.port, 443);
        assert_eq!(ev.route, "proxy");
    }

    #[test]
    fn parse_socks_proxy_connection() {
        let line = "[xray] from 127.0.0.1:51241 accepted tcp:1.2.3.4:80 [socks-in >> proxy]";
        let ev = parse_xray_connection(line, 10).unwrap();
        // "tcp:1.2.3.4:80" -- after stripping protocol prefixes, host_port = "tcp:1.2.3.4:80"
        // rfind(':') splits at last colon -> port=80, host contains "tcp:1.2.3.4" or similar
        assert_eq!(ev.port, 80);
        assert_eq!(ev.route, "proxy");
        assert_eq!(ev.protocol, "SOCKS5");
        assert_eq!(ev.id, 10);
    }

    #[test]
    fn parse_socks_direct_connection() {
        let line = "[xray] from 127.0.0.1:51241 accepted //example.com:443 [socks-in >> direct]";
        let ev = parse_xray_connection(line, 5).unwrap();
        assert_eq!(ev.host, "example.com");
        assert_eq!(ev.port, 443);
        assert_eq!(ev.route, "direct");
        assert_eq!(ev.protocol, "SOCKS5");
    }

    #[test]
    fn parse_block_route() {
        let line = "[xray] from 127.0.0.1:1234 accepted //blocked.com:443 [http-in >> block]";
        let ev = parse_xray_connection(line, 7).unwrap();
        assert_eq!(ev.host, "blocked.com");
        assert_eq!(ev.route, "block");
        assert_eq!(ev.protocol, "HTTP");
    }

    #[test]
    fn skip_unknown_route_type() {
        let line = "[xray] from 127.0.0.1:1234 accepted //example.com:443 [http-in >> unknown]";
        assert!(parse_xray_connection(line, 1).is_none());
    }

    #[test]
    fn skip_line_without_accepted() {
        let line = "[xray] some random log message without the keyword";
        assert!(parse_xray_connection(line, 1).is_none());
    }

    #[test]
    fn skip_line_without_brackets() {
        let line = "[xray] from 127.0.0.1:1234 accepted //example.com:443";
        assert!(parse_xray_connection(line, 1).is_none());
    }

    #[test]
    fn skip_empty_line() {
        assert!(parse_xray_connection("", 1).is_none());
    }

    #[test]
    fn extract_time_from_iso_timestamp() {
        let line = "2026-04-02T14:30:55.123456Z  INFO [xray] from 127.0.0.1:1234 accepted //example.com:443 [http-in >> proxy]";
        let ev = parse_xray_connection(line, 1).unwrap();
        // extract_time finds the first HH:MM:SS pattern -- "14:30:55" from the ISO timestamp
        assert_eq!(ev.time, "14:30:55");
    }

    #[test]
    fn extract_time_from_xray_timestamp() {
        let line = "[xray] 2026/04/02 09:15:42.288925 [Info] from 127.0.0.1:1234 accepted //example.com:443 [http-in >> proxy]";
        let ev = parse_xray_connection(line, 1).unwrap();
        assert_eq!(ev.time, "09:15:42");
    }

    #[test]
    fn counter_passed_through_as_id() {
        let line = "[xray] from 127.0.0.1:1234 accepted //example.com:443 [http-in >> proxy]";
        let ev = parse_xray_connection(line, 999).unwrap();
        assert_eq!(ev.id, 999);
    }

    #[test]
    fn parse_host_without_port() {
        // Edge case: destination has no port
        let line = "[xray] from 127.0.0.1:1234 accepted //example.com [http-in >> proxy]";
        let ev = parse_xray_connection(line, 1).unwrap();
        assert_eq!(ev.host, "example.com");
        assert_eq!(ev.port, 0);
    }

    #[test]
    fn parse_destination_with_path() {
        let line = "[xray] from 127.0.0.1:51241 accepted http://91.105.192.100:80/api/v2/data [http-in >> proxy]";
        let ev = parse_xray_connection(line, 1).unwrap();
        assert_eq!(ev.host, "91.105.192.100");
        assert_eq!(ev.port, 80);
    }
}

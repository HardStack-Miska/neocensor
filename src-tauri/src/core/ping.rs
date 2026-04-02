use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::net::TcpStream;

/// TCP handshake ping to a server.
/// Returns latency in milliseconds.
pub async fn tcp_ping(host: &str, port: u16, timeout: Duration) -> Result<u32> {
    let addr = format!("{host}:{port}");
    let start = Instant::now();

    tokio::time::timeout(timeout, TcpStream::connect(&addr)).await??;

    Ok(start.elapsed().as_millis() as u32)
}

/// Ping multiple servers in parallel. Returns (index, result) pairs.
pub async fn ping_all(
    servers: &[(String, u16)],
    timeout: Duration,
) -> Vec<(usize, Result<u32>)> {
    let futures: Vec<_> = servers
        .iter()
        .enumerate()
        .map(|(i, (host, port))| {
            let host = host.clone();
            let port = *port;
            async move { (i, tcp_ping(&host, port, timeout).await) }
        })
        .collect();

    futures::future::join_all(futures).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Network-dependent, may pass on WSL due to instant connect
    async fn ping_unreachable_host() {
        let result = tcp_ping("192.0.2.1", 12345, Duration::from_millis(1)).await;
        assert!(result.is_err());
    }
}

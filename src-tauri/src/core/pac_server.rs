use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

/// Minimal HTTP server that serves a PAC (Proxy Auto-Config) file.
/// The PAC returns "PROXY host:port; DIRECT" — this means apps will try
/// the proxy first, and fall back to direct if the proxy is unreachable.
/// Combined with WFP blocking proxy port for DIRECT apps, this achieves
/// per-process routing.
pub struct PacServer {
    shutdown_tx: Option<watch::Sender<bool>>,
    port: u16,
}

impl PacServer {
    pub fn new() -> Self {
        Self {
            shutdown_tx: None,
            port: 0,
        }
    }

    /// Start serving the PAC file. Returns the port the server is listening on.
    pub async fn start(&mut self, proxy_host: &str, proxy_port: u16) -> Result<u16> {
        self.stop().await;

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        self.port = port;

        let pac_content = generate_pac(proxy_host, proxy_port);
        let pac_response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/x-ns-proxy-autoconfig\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            pac_content.len(),
            pac_content
        );
        let response_bytes = Arc::new(pac_response.into_bytes());

        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx);

        tracing::info!("PAC server started on 127.0.0.1:{port}");

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((mut stream, addr)) => {
                                tracing::trace!(
                                    "PAC request from {}",
                                    addr
                                );
                                let resp = response_bytes.clone();
                                tokio::spawn(async move {
                                    let mut buf = [0u8; 1024];
                                    // Read the HTTP request
                                    let n = stream.read(&mut buf).await.unwrap_or(0);
                                    if n > 0 {
                                        // Extract first line of HTTP request for logging
                                        let request_str = String::from_utf8_lossy(&buf[..n]);
                                        if let Some(first_line) = request_str.lines().next() {
                                            tracing::trace!(
                                                "PAC request from {}: {}",
                                                addr,
                                                first_line
                                            );
                                        }
                                    }
                                    // Send PAC response
                                    let _ = stream.write_all(&resp).await;
                                    let _ = stream.shutdown().await;
                                });
                            }
                            Err(e) => {
                                tracing::warn!("PAC server accept error: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        tracing::info!("PAC server shutting down");
                        break;
                    }
                }
            }
        });

        Ok(port)
    }

    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
        self.port = 0;
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Generate PAC file content.
/// The key: `PROXY host:port; DIRECT` means "try proxy, fallback to direct".
/// When WFP blocks a DIRECT app's connection to proxy port, the browser
/// gets connection refused and falls back to DIRECT automatically.
fn generate_pac(proxy_host: &str, proxy_port: u16) -> String {
    let conditions: Vec<String> = crate::utils::PRIVATE_IP_RANGES
        .iter()
        .map(|r| format!("        shExpMatch(host, \"{r}\")"))
        .collect();
    let conditions_str = conditions.join(" ||\n");

    format!(
        r#"function FindProxyForURL(url, host) {{
    // Don't proxy local addresses
    if (isPlainHostName(host) ||
{conditions_str}) {{
        return "DIRECT";
    }}
    return "PROXY {proxy_host}:{proxy_port}; DIRECT";
}}"#
    )
}

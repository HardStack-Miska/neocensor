use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, broadcast};

/// Manages the xray-core child process lifecycle.
pub struct XrayManager {
    binary_path: PathBuf,
    config_path: PathBuf,
    child: Arc<Mutex<Option<Child>>>,
    log_sender: broadcast::Sender<String>,
}

impl XrayManager {
    pub fn new(binary_path: PathBuf, config_dir: PathBuf) -> Self {
        let (log_sender, _) = broadcast::channel(1024);
        Self {
            binary_path,
            config_path: config_dir.join("xray-config.json"),
            child: Arc::new(Mutex::new(None)),
            log_sender,
        }
    }

    /// Subscribe to xray-core log output.
    pub fn subscribe_logs(&self) -> broadcast::Receiver<String> {
        self.log_sender.subscribe()
    }

    /// Write config and start xray-core.
    pub async fn start(&self, config: &serde_json::Value) -> Result<()> {
        self.stop().await?;

        let config_str = serde_json::to_string_pretty(config)?;
        tokio::fs::write(&self.config_path, &config_str)
            .await
            .context("failed to write xray config")?;

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("run")
            .arg("-config")
            .arg(&self.config_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        // Hide console window on Windows
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().context("failed to start xray-core")?;

        // Stream stderr to tracing + broadcast
        if let Some(stderr) = child.stderr.take() {
            let sender = self.log_sender.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!("[xray] {line}");
                    let _ = sender.send(format!("[xray] {line}"));
                }
            });
        }

        // Stream stdout to tracing + broadcast
        if let Some(stdout) = child.stdout.take() {
            let sender = self.log_sender.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    // Downgrade expected connection-reset noise to debug
                    if line.contains("connection ends") || line.contains("wsarecv") || line.contains("wsasend") {
                        tracing::debug!("[xray] {line}");
                    } else {
                        tracing::info!("[xray] {line}");
                    }
                    let _ = sender.send(format!("[xray] {line}"));
                }
            });
        }

        *self.child.lock().await = Some(child);
        tracing::info!("xray-core started");
        Ok(())
    }

    /// Stop xray-core gracefully and remove config file (contains credentials).
    pub async fn stop(&self) -> Result<()> {
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            child.kill().await.ok();
            child.wait().await.ok();
            tracing::info!("xray-core stopped");
        }
        // Remove config file to avoid leaving credentials on disk
        tokio::fs::remove_file(&self.config_path).await.ok();
        Ok(())
    }

    /// Restart with a new config.
    pub async fn restart(&self, config: &serde_json::Value) -> Result<()> {
        self.stop().await?;
        // Brief pause to release port
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        self.start(config).await
    }

    /// Check if xray-core process is still running.
    pub async fn is_alive(&self) -> bool {
        let mut guard = self.child.lock().await;
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => true,  // Still running
                Ok(Some(_)) => {
                    *guard = None;
                    false
                }
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Get the path to the xray-core binary.
    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }

    /// Get the path to the current config file.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }
}

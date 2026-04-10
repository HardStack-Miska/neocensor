use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, broadcast};

/// Manages the sing-box child process lifecycle.
pub struct SingboxManager {
    binary_path: PathBuf,
    config_path: PathBuf,
    child: Arc<Mutex<Option<Child>>>,
    log_sender: broadcast::Sender<String>,
}

impl SingboxManager {
    pub fn new(binary_path: PathBuf, config_dir: PathBuf) -> Self {
        let (log_sender, _) = broadcast::channel(1024);
        Self {
            binary_path,
            config_path: config_dir.join("singbox-config.json"),
            child: Arc::new(Mutex::new(None)),
            log_sender,
        }
    }

    /// Subscribe to sing-box log output.
    pub fn subscribe_logs(&self) -> broadcast::Receiver<String> {
        self.log_sender.subscribe()
    }

    /// Write config and start sing-box.
    pub async fn start(&self, config: &serde_json::Value) -> Result<()> {
        self.stop().await?;
        // Brief pause to let OS release bound ports from previous instance
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let config_str = serde_json::to_string_pretty(config)?;
        tokio::fs::write(&self.config_path, &config_str)
            .await
            .context("failed to write sing-box config")?;

        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("run")
            .arg("-c")
            .arg(&self.config_path)
            .arg("--disable-color")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        // Hide console window and prevent conhost creation on Windows
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            const DETACHED_PROCESS: u32 = 0x00000008;
            cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
        }

        let mut child = cmd.spawn().context("failed to start sing-box")?;

        // Stream stderr to tracing + broadcast (sing-box logs primarily to stderr)
        if let Some(stderr) = child.stderr.take() {
            let sender = self.log_sender.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    // Downgrade noisy connection log lines to debug
                    if line.contains("connection") {
                        tracing::debug!("[sing-box] {line}");
                    } else {
                        tracing::info!("[sing-box] {line}");
                    }
                    let _ = sender.send(format!("[sing-box] {line}"));
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
                    tracing::info!("[sing-box] {line}");
                    let _ = sender.send(format!("[sing-box] {line}"));
                }
            });
        }

        *self.child.lock().await = Some(child);
        tracing::info!("sing-box started");
        Ok(())
    }

    /// Stop sing-box gracefully and remove config file (contains credentials).
    pub async fn stop(&self) -> Result<()> {
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            // Try graceful kill first
            child.kill().await.ok();
            // Wait with timeout to avoid hanging on zombie process
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                child.wait(),
            ).await;
            tracing::info!("sing-box stopped");
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

    /// Check if sing-box process is still running.
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

    /// Get the path to the sing-box binary.
    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }

    /// Get the path to the current config file.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }
}

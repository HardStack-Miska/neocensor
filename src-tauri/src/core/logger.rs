use std::path::Path;

use tokio::sync::broadcast;
use tracing_subscriber::fmt::MakeWriter;

/// In-memory ring buffer that also forwards logs to a broadcast channel
/// so the frontend can subscribe to live log output.
pub struct LogBroadcaster {
    sender: broadcast::Sender<String>,
}

impl LogBroadcaster {
    pub fn new() -> (Self, broadcast::Receiver<String>) {
        let (sender, receiver) = broadcast::channel(2048);
        (Self { sender }, receiver)
    }

    pub fn sender(&self) -> broadcast::Sender<String> {
        self.sender.clone()
    }
}

/// Initialize tracing with both stdout and file logging.
/// Returns a broadcast sender for forwarding logs to the frontend.
pub fn init_logging(log_dir: &Path) -> broadcast::Sender<String> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    std::fs::create_dir_all(log_dir).ok();

    let (broadcaster, _rx) = LogBroadcaster::new();
    let sender = broadcaster.sender();
    let sender_clone = sender.clone();

    // File appender — daily rotation, keep 14 days max
    let file_appender = tracing_appender::rolling::Builder::new()
        .filename_prefix("neocensor")
        .filename_suffix("log")
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .max_log_files(14)
        .build(log_dir)
        .expect("failed to build rolling file appender");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    // Leak guard so it lives for the process lifetime
    std::mem::forget(_guard);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("neocensor=debug,warn"));

    // Broadcast layer: sends formatted log lines to the channel
    let broadcast_layer = fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .compact()
        .with_writer(BroadcastWriter(sender_clone));

    // File layer
    let file_layer = fmt::layer()
        .with_target(true)
        .with_ansi(false)
        .with_writer(file_writer);

    // Stdout layer (dev only)
    let stdout_layer = fmt::layer()
        .with_target(false)
        .compact();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .with(broadcast_layer)
        .init();

    sender
}

/// A tracing writer that sends each line to a broadcast channel.
#[derive(Clone)]
struct BroadcastWriter(broadcast::Sender<String>);

impl std::io::Write for BroadcastWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(s) = std::str::from_utf8(buf) {
            let trimmed = s.trim_end();
            if !trimmed.is_empty() {
                let _ = self.0.send(trimmed.to_string());
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BroadcastWriter {
    type Writer = BroadcastWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

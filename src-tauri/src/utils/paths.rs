use std::path::PathBuf;

use anyhow::{Context, Result};

/// Get the NeoCensor data directory: %APPDATA%/NeoCensor (Windows)
/// or ~/.local/share/neocensor (Linux).
pub fn data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("failed to determine data directory")?;
    let dir = base.join("NeoCensor");
    Ok(dir)
}

/// Get the config directory inside data dir.
pub fn config_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("config"))
}

/// Get the logs directory.
pub fn logs_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("logs"))
}

/// Get the geo-rules directory.
pub fn geo_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("geo"))
}

/// Get the icons cache directory.
pub fn icons_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("icons"))
}

/// Ensure all required directories exist.
pub async fn ensure_dirs() -> Result<()> {
    for dir in [data_dir()?, config_dir()?, logs_dir()?, geo_dir()?, icons_dir()?] {
        tokio::fs::create_dir_all(&dir).await?;
    }
    Ok(())
}

/// Get the path where xray-core binary should be.
pub fn xray_binary_path() -> Result<PathBuf> {
    let dir = data_dir()?.join("bin");
    #[cfg(windows)]
    let name = "xray.exe";
    #[cfg(not(windows))]
    let name = "xray";
    Ok(dir.join(name))
}

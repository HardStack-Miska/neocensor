use tauri::{Emitter, State};

use crate::app_state::ManagedState;
use crate::core::downloader;

#[derive(Clone, serde::Serialize)]
struct DownloadProgress {
    component: String,
    status: String,
}

#[tauri::command]
pub async fn check_binaries(
    state: State<'_, ManagedState>,
) -> Result<BinaryStatus, String> {
    let singbox_exists = state.singbox.binary_path().exists();

    Ok(BinaryStatus {
        singbox_installed: singbox_exists,
    })
}

#[derive(serde::Serialize)]
pub struct BinaryStatus {
    pub singbox_installed: bool,
}

#[tauri::command]
pub async fn download_components(
    state: State<'_, ManagedState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let bin_dir = state.singbox.binary_path()
        .parent()
        .ok_or("sing-box binary path has no parent directory")?
        .to_path_buf();
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;

    // Download sing-box
    if !state.singbox.binary_path().exists() {
        let _ = app_handle.emit(
            "download-progress",
            DownloadProgress {
                component: "sing-box".into(),
                status: "downloading".into(),
            },
        );

        let version = downloader::check_latest_version("SagerNet/sing-box")
            .await
            .unwrap_or_else(|_| "1.11.0".into());

        downloader::download_singbox(&version, &bin_dir)
            .await
            .map_err(|e| format!("sing-box download failed: {e}"))?;

        let _ = app_handle.emit(
            "download-progress",
            DownloadProgress {
                component: "sing-box".into(),
                status: "installed".into(),
            },
        );
    }

    Ok(())
}

#[tauri::command]
pub async fn check_latest_versions() -> Result<ComponentVersions, String> {
    let singbox = downloader::check_latest_version("SagerNet/sing-box")
        .await
        .unwrap_or_else(|_| "unknown".into());

    Ok(ComponentVersions {
        singbox_latest: singbox,
    })
}

#[derive(serde::Serialize)]
pub struct ComponentVersions {
    pub singbox_latest: String,
}

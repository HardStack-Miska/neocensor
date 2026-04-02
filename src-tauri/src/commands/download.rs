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
    let xray_exists = state.xray.binary_path().exists();

    Ok(BinaryStatus {
        xray_installed: xray_exists,
    })
}

#[derive(serde::Serialize)]
pub struct BinaryStatus {
    pub xray_installed: bool,
}

#[tauri::command]
pub async fn download_components(
    state: State<'_, ManagedState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let bin_dir = state.xray.binary_path()
        .parent()
        .ok_or("xray binary path has no parent directory")?
        .to_path_buf();
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;

    // Download xray-core
    if !state.xray.binary_path().exists() {
        let _ = app_handle.emit(
            "download-progress",
            DownloadProgress {
                component: "xray-core".into(),
                status: "downloading".into(),
            },
        );

        let version = downloader::check_latest_version("XTLS/Xray-core")
            .await
            .unwrap_or_else(|_| "25.3.6".into());

        downloader::download_xray(&version, &bin_dir)
            .await
            .map_err(|e| format!("xray download failed: {e}"))?;

        let _ = app_handle.emit(
            "download-progress",
            DownloadProgress {
                component: "xray-core".into(),
                status: "installed".into(),
            },
        );
    }

    Ok(())
}

#[tauri::command]
pub async fn check_latest_versions() -> Result<ComponentVersions, String> {
    let xray = downloader::check_latest_version("XTLS/Xray-core")
        .await
        .unwrap_or_else(|_| "unknown".into());

    Ok(ComponentVersions {
        xray_latest: xray,
    })
}

#[derive(serde::Serialize)]
pub struct ComponentVersions {
    pub xray_latest: String,
}

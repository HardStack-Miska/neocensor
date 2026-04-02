use anyhow::{anyhow, Result};

/// Get the WFP Application ID blob for a given executable path.
#[cfg(windows)]
pub fn get_app_id(exe_path: &str) -> Result<AppIdBlob> {
    use windows::core::PCWSTR;
    use windows::Win32::NetworkManagement::WindowsFilteringPlatform::{
        FwpmFreeMemory0, FwpmGetAppIdFromFileName0, FWP_BYTE_BLOB,
    };

    let wide_path: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let mut blob_ptr: *mut FWP_BYTE_BLOB = std::ptr::null_mut();

        let ret = FwpmGetAppIdFromFileName0(
            PCWSTR(wide_path.as_ptr()),
            &mut blob_ptr,
        );
        if ret != 0 {
            return Err(anyhow!("FwpmGetAppIdFromFileName0 failed: error {ret:#X}"));
        }

        if blob_ptr.is_null() {
            return Err(anyhow!("FwpmGetAppIdFromFileName0 returned null"));
        }

        let blob = &*blob_ptr;
        let data = std::slice::from_raw_parts(blob.data, blob.size as usize).to_vec();

        // Free the memory allocated by WFP
        FwpmFreeMemory0(&mut (blob_ptr as *mut _));

        Ok(AppIdBlob { data })
    }
}

#[cfg(not(windows))]
pub fn get_app_id(_exe_path: &str) -> Result<AppIdBlob> {
    anyhow::bail!("WFP not available on this platform")
}

/// Owned copy of a WFP Application ID blob.
#[derive(Debug, Clone)]
pub struct AppIdBlob {
    pub data: Vec<u8>,
}

/// Resolve a process name to its full executable path by scanning running processes.
pub fn resolve_exe_path(process_name: &str, monitor: &mut crate::core::process_monitor::ProcessMonitor) -> Option<String> {
    let map = monitor.build_exe_map();
    resolve_exe_path_from_map(process_name, &map)
}

/// Resolve exe path using a pre-built map (avoids repeated full process scans).
pub fn resolve_exe_path_from_map(
    process_name: &str,
    exe_map: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let result = exe_map
        .get(&process_name.to_lowercase())
        .cloned();
    match &result {
        Some(path) => tracing::debug!("resolve_exe_path: found {} -> {}", process_name, path),
        None => tracing::debug!("resolve_exe_path: not found for {}", process_name),
    }
    result
}

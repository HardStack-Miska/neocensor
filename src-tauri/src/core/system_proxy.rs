//! Minimal system proxy for non-admin fallback mode (when TUN is unavailable).
//! Uses Windows Registry API directly — no subprocess, no WSL notifications.
//!
//! Backs up the user's prior proxy settings to a sidecar file before overwriting,
//! so a crash without `unset_system_proxy` can be self-healed on next launch.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

const PROXY_BYPASS: &str =
    "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;<local>";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProxyBackup {
    enable: u32,
    server: Option<String>,
    bypass: Option<String>,
    pac_url: Option<String>,
}

fn backup_path() -> Result<PathBuf> {
    Ok(crate::utils::data_dir()?.join("proxy_backup.json"))
}

#[cfg(windows)]
pub fn set_system_proxy(host: &str, port: u16) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinInet::InternetSetOptionW;
    use windows::Win32::System::Registry::*;

    let proxy = format!("{host}:{port}");
    tracing::info!("setting system proxy to {proxy} (fallback mode)");

    // Save existing settings before overwriting (only if no backup exists yet).
    if let Err(e) = save_backup_if_missing() {
        tracing::warn!("failed to backup prior proxy settings: {e}");
    }

    let subkey: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let mut hkey = HKEY::default();
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            REG_SAM_FLAGS(KEY_SET_VALUE.0),
            &mut hkey,
        )
        .ok()?;

        let name_enable: Vec<u16> = "ProxyEnable"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let val: u32 = 1;
        RegSetValueExW(
            hkey,
            PCWSTR(name_enable.as_ptr()),
            None,
            REG_DWORD,
            Some(&val.to_le_bytes()),
        )
        .ok()?;

        write_string_value(hkey, "ProxyServer", &proxy)?;
        write_string_value(hkey, "ProxyOverride", PROXY_BYPASS)?;

        let _ = RegCloseKey(hkey);
        InternetSetOptionW(None, 39, None, 0).ok(); // SETTINGS_CHANGED
        InternetSetOptionW(None, 37, None, 0).ok(); // REFRESH
    }
    Ok(())
}

#[cfg(windows)]
pub fn unset_system_proxy() -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinInet::InternetSetOptionW;
    use windows::Win32::System::Registry::*;

    tracing::info!("removing system proxy");

    // Try to restore backup first
    let backup = read_backup().ok().flatten();

    let subkey: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let mut hkey = HKEY::default();
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            REG_SAM_FLAGS(KEY_SET_VALUE.0),
            &mut hkey,
        )
        .ok()?;

        // Restore prior ProxyEnable (default: 0 = disabled)
        let enable_val = backup.as_ref().map(|b| b.enable).unwrap_or(0);
        let name_enable: Vec<u16> = "ProxyEnable"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        RegSetValueExW(
            hkey,
            PCWSTR(name_enable.as_ptr()),
            None,
            REG_DWORD,
            Some(&enable_val.to_le_bytes()),
        )
        .ok()?;

        // Restore prior ProxyServer / ProxyOverride if they existed
        if let Some(b) = backup.as_ref() {
            if let Some(server) = &b.server {
                let _ = write_string_value(hkey, "ProxyServer", server);
            } else {
                let _ = delete_value(hkey, "ProxyServer");
            }
            if let Some(bypass) = &b.bypass {
                let _ = write_string_value(hkey, "ProxyOverride", bypass);
            } else {
                let _ = delete_value(hkey, "ProxyOverride");
            }
            if let Some(pac) = &b.pac_url {
                let _ = write_string_value(hkey, "AutoConfigURL", pac);
            }
        } else {
            // No backup → just clear our values
            let _ = delete_value(hkey, "ProxyServer");
            let _ = delete_value(hkey, "ProxyOverride");
        }

        let _ = RegCloseKey(hkey);
        InternetSetOptionW(None, 39, None, 0).ok();
        InternetSetOptionW(None, 37, None, 0).ok();
    }

    // Remove backup sidecar after successful restore
    if let Ok(p) = backup_path() {
        let _ = std::fs::remove_file(&p);
    }
    Ok(())
}

/// Detect a stale system proxy pointing at our mixed port and restore on app startup.
/// Called once on launch to recover from a crash that left ProxyEnable=1.
///
/// Matches `127.0.0.1:<expected_port>` EXACTLY — never the loose `127.0.0.1:*`
/// pattern (would otherwise clobber legit local proxies like Burp/Fiddler/Charles
/// that the user may have configured outside of NeoCensor).
#[cfg(windows)]
pub fn restore_if_orphaned(expected_port: u16) -> Result<bool> {
    let bp = backup_path()?;
    if !bp.exists() {
        return Ok(false);
    }

    let expected = format!("127.0.0.1:{expected_port}");
    let current = read_current_proxy().ok();
    let is_ours = current
        .as_ref()
        .and_then(|c| c.server.as_deref())
        .map(|s| s == expected)
        .unwrap_or(false);

    let was_enabled = current.as_ref().map(|c| c.enable == 1).unwrap_or(false);

    if is_ours && was_enabled {
        tracing::warn!(
            "detected orphaned system proxy ({expected}) from previous session, restoring backup"
        );
        unset_system_proxy()?;
        return Ok(true);
    }

    // Backup exists but registry doesn't point at us — drop the stale backup so a
    // future legitimate set/unset cycle can capture an up-to-date snapshot.
    let _ = std::fs::remove_file(&bp);
    Ok(false)
}

#[cfg(windows)]
fn save_backup_if_missing() -> Result<()> {
    let path = backup_path()?;
    if path.exists() {
        return Ok(());
    }
    let current = read_current_proxy()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_string_pretty(&current)?;
    std::fs::write(&path, json)?;
    Ok(())
}

#[cfg(windows)]
fn read_backup() -> Result<Option<ProxyBackup>> {
    let path = backup_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&text).ok())
}

#[cfg(windows)]
fn read_current_proxy() -> Result<ProxyBackup> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::*;

    let subkey: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let mut hkey = HKEY::default();
    let mut backup = ProxyBackup::default();

    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            REG_SAM_FLAGS(KEY_QUERY_VALUE.0),
            &mut hkey,
        )
        .ok()?;

        backup.enable = read_dword_value(hkey, "ProxyEnable").unwrap_or(0);
        backup.server = read_string_value(hkey, "ProxyServer");
        backup.bypass = read_string_value(hkey, "ProxyOverride");
        backup.pac_url = read_string_value(hkey, "AutoConfigURL");

        let _ = RegCloseKey(hkey);
    }
    Ok(backup)
}

#[cfg(windows)]
unsafe fn write_string_value(
    hkey: windows::Win32::System::Registry::HKEY,
    name: &str,
    value: &str,
) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::*;

    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let val_w: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let val_bytes =
        std::slice::from_raw_parts(val_w.as_ptr() as *const u8, val_w.len() * 2);
    RegSetValueExW(
        hkey,
        PCWSTR(name_w.as_ptr()),
        None,
        REG_SZ,
        Some(val_bytes),
    )
    .ok()?;
    Ok(())
}

#[cfg(windows)]
unsafe fn delete_value(
    hkey: windows::Win32::System::Registry::HKEY,
    name: &str,
) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::*;

    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    RegDeleteValueW(hkey, PCWSTR(name_w.as_ptr())).ok()?;
    Ok(())
}

#[cfg(windows)]
unsafe fn read_dword_value(
    hkey: windows::Win32::System::Registry::HKEY,
    name: &str,
) -> Option<u32> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::*;

    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut data: u32 = 0;
    let mut data_size = std::mem::size_of::<u32>() as u32;
    let mut reg_type = REG_VALUE_TYPE::default();

    let result = RegQueryValueExW(
        hkey,
        PCWSTR(name_w.as_ptr()),
        None,
        Some(&mut reg_type),
        Some(&mut data as *mut u32 as *mut u8),
        Some(&mut data_size),
    );

    if result.is_ok() && reg_type == REG_DWORD {
        Some(data)
    } else {
        None
    }
}

#[cfg(windows)]
unsafe fn read_string_value(
    hkey: windows::Win32::System::Registry::HKEY,
    name: &str,
) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::*;

    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut data_size: u32 = 0;
    let mut reg_type = REG_VALUE_TYPE::default();

    // Query size first
    let size_result = RegQueryValueExW(
        hkey,
        PCWSTR(name_w.as_ptr()),
        None,
        Some(&mut reg_type),
        None,
        Some(&mut data_size),
    );

    if !size_result.is_ok() || data_size == 0 {
        return None;
    }
    if reg_type != REG_SZ && reg_type != REG_EXPAND_SZ {
        return None;
    }

    let mut buf = vec![0u8; data_size as usize];
    let result = RegQueryValueExW(
        hkey,
        PCWSTR(name_w.as_ptr()),
        None,
        Some(&mut reg_type),
        Some(buf.as_mut_ptr()),
        Some(&mut data_size),
    );

    if !result.is_ok() {
        return None;
    }

    // Convert UTF-16 to String, dropping trailing NUL
    let len_chars = (data_size as usize) / 2;
    let utf16: Vec<u16> = (0..len_chars)
        .map(|i| u16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]))
        .collect();
    let trimmed: Vec<u16> = utf16.into_iter().take_while(|&c| c != 0).collect();
    if trimmed.is_empty() {
        None
    } else {
        Some(String::from_utf16_lossy(&trimmed))
    }
}

#[cfg(not(windows))]
pub fn set_system_proxy(_host: &str, _port: u16) -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn unset_system_proxy() -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn restore_if_orphaned(_expected_port: u16) -> Result<bool> {
    Ok(false)
}

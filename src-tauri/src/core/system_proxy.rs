use anyhow::{bail, Result};

/// Validate that host is a safe hostname/IP.
fn validate_proxy_host(host: &str) -> Result<()> {
    if host.is_empty() {
        bail!("proxy host is empty");
    }
    if !host.chars().all(|c| c.is_ascii_alphanumeric() || ".:-[]".contains(c)) {
        bail!("proxy host contains invalid characters: {host}");
    }
    Ok(())
}

// ─── Windows implementation using Registry API directly ───
// No reg.exe or powershell — no console windows, no WSL notifications from subprocess spawning.

#[cfg(windows)]
mod win {
    use anyhow::{Context, Result};
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, KEY_QUERY_VALUE,
        REG_DWORD, REG_SZ, RegQueryValueExW,
    };
    use windows::Win32::Networking::WinInet::InternetSetOptionW;

    const INTERNET_SETTINGS: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
    const INTERNET_OPTION_REFRESH: u32 = 37;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open_key(access: u32) -> Result<HKEY> {
        let subkey = wide(INTERNET_SETTINGS);
        let mut hkey = HKEY::default();
        unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                windows::Win32::System::Registry::REG_SAM_FLAGS(access),
                &mut hkey,
            )
            .ok()
            .context("failed to open Internet Settings registry key")?;
        }
        Ok(hkey)
    }

    fn set_dword(hkey: HKEY, name: &str, value: u32) -> Result<()> {
        let name_w = wide(name);
        let bytes = value.to_le_bytes();
        unsafe {
            RegSetValueExW(
                hkey,
                PCWSTR(name_w.as_ptr()),
                None,
                REG_DWORD,
                Some(&bytes),
            )
            .ok()
            .context(format!("failed to set {name}"))?;
        }
        Ok(())
    }

    fn set_string(hkey: HKEY, name: &str, value: &str) -> Result<()> {
        let name_w = wide(name);
        let value_w = wide(value);
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                value_w.as_ptr() as *const u8,
                value_w.len() * 2,
            )
        };
        unsafe {
            RegSetValueExW(
                hkey,
                PCWSTR(name_w.as_ptr()),
                None,
                REG_SZ,
                Some(bytes),
            )
            .ok()
            .context(format!("failed to set {name}"))?;
        }
        Ok(())
    }

    fn delete_value(hkey: HKEY, name: &str) {
        let name_w = wide(name);
        unsafe {
            let _ = RegDeleteValueW(hkey, PCWSTR(name_w.as_ptr()));
        }
    }

    fn read_dword(hkey: HKEY, name: &str) -> Option<u32> {
        let name_w = wide(name);
        let mut data = [0u8; 4];
        let mut size = 4u32;
        let mut kind = REG_DWORD;
        unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR(name_w.as_ptr()),
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr()),
                Some(&mut size),
            )
            .ok()
            .ok()?;
        }
        Some(u32::from_le_bytes(data))
    }

    fn read_string(hkey: HKEY, name: &str) -> Option<String> {
        let name_w = wide(name);
        let mut size = 0u32;
        let mut kind = REG_SZ;
        // First call to get size
        unsafe {
            let _ = RegQueryValueExW(
                hkey,
                PCWSTR(name_w.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            );
        }
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR(name_w.as_ptr()),
                None,
                Some(&mut kind),
                Some(buf.as_mut_ptr()),
                Some(&mut size),
            )
            .ok()
            .ok()?;
        }
        // Convert UTF-16LE bytes to String
        let wide_slice: &[u16] = unsafe {
            std::slice::from_raw_parts(buf.as_ptr() as *const u16, (size as usize) / 2)
        };
        // Trim null terminator
        let len = wide_slice.iter().position(|&c| c == 0).unwrap_or(wide_slice.len());
        Some(String::from_utf16_lossy(&wide_slice[..len]))
    }

    fn close_key(hkey: HKEY) {
        unsafe { let _ = RegCloseKey(hkey); }
    }

    /// Notify Windows that proxy settings changed (InternetSetOption).
    fn notify() {
        unsafe {
            let _ = InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED, None, 0);
            let _ = InternetSetOptionW(None, INTERNET_OPTION_REFRESH, None, 0);
        }
    }

    pub fn set_proxy_with_pac(proxy: &str, pac_url: &str, override_str: &str) -> Result<()> {
        let hkey = open_key(KEY_SET_VALUE.0)?;
        set_dword(hkey, "ProxyEnable", 1)?;
        set_string(hkey, "ProxyServer", proxy)?;
        set_string(hkey, "ProxyOverride", override_str)?;
        set_string(hkey, "AutoConfigURL", pac_url)?;
        close_key(hkey);
        notify();
        Ok(())
    }

    pub fn set_proxy(proxy: &str, override_str: &str) -> Result<()> {
        let hkey = open_key(KEY_SET_VALUE.0)?;
        set_dword(hkey, "ProxyEnable", 1)?;
        set_string(hkey, "ProxyServer", proxy)?;
        set_string(hkey, "ProxyOverride", override_str)?;
        close_key(hkey);
        notify();
        Ok(())
    }

    pub fn unset_proxy() -> Result<()> {
        let hkey = open_key(KEY_SET_VALUE.0)?;
        set_dword(hkey, "ProxyEnable", 0)?;
        delete_value(hkey, "AutoConfigURL");
        close_key(hkey);
        notify();
        Ok(())
    }

    pub fn check_stale(default_port: u16) -> bool {
        let hkey = match open_key(KEY_QUERY_VALUE.0) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let enabled = read_dword(hkey, "ProxyEnable").unwrap_or(0) == 1;
        if !enabled {
            close_key(hkey);
            return false;
        }
        let server = read_string(hkey, "ProxyServer").unwrap_or_default();
        close_key(hkey);
        server.contains(&format!("127.0.0.1:{default_port}"))
    }
}

// ─── Public API ───

#[cfg(windows)]
pub fn set_system_proxy_with_pac(host: &str, port: u16, pac_port: u16) -> Result<()> {
    validate_proxy_host(host)?;
    let proxy = format!("{host}:{port}");
    let pac_url = format!("http://127.0.0.1:{pac_port}/proxy.pac");
    let override_str = crate::utils::proxy_override_string();
    tracing::info!("setting system proxy to {proxy} + PAC {pac_url}");
    win::set_proxy_with_pac(&proxy, &pac_url, &override_str)?;
    tracing::info!("system proxy set to {proxy} (+ PAC fallback)");
    Ok(())
}

#[cfg(windows)]
pub fn set_system_proxy(host: &str, port: u16) -> Result<()> {
    validate_proxy_host(host)?;
    let proxy = format!("{host}:{port}");
    let override_str = crate::utils::proxy_override_string();
    tracing::info!("setting system proxy to {proxy}");
    win::set_proxy(&proxy, &override_str)?;
    tracing::info!("system proxy set to {proxy}");
    Ok(())
}

#[cfg(windows)]
pub fn unset_system_proxy() -> Result<()> {
    tracing::info!("removing system proxy");
    win::unset_proxy()?;
    tracing::info!("system proxy removed");
    Ok(())
}

#[cfg(windows)]
pub fn cleanup_stale_proxy(default_port: u16) {
    if win::check_stale(default_port) {
        tracing::warn!("detected stale proxy settings from previous session, cleaning up");
        if let Err(e) = unset_system_proxy() {
            tracing::error!("failed to clean stale proxy: {e}");
        }
    }
}

// ─── Non-Windows stubs ───

#[cfg(not(windows))]
pub fn set_system_proxy_with_pac(_host: &str, _port: u16, _pac_port: u16) -> Result<()> {
    tracing::warn!("system proxy not implemented on this platform");
    Ok(())
}

#[cfg(not(windows))]
pub fn set_system_proxy(_host: &str, _port: u16) -> Result<()> {
    tracing::warn!("system proxy not implemented on this platform");
    Ok(())
}

#[cfg(not(windows))]
pub fn unset_system_proxy() -> Result<()> {
    tracing::warn!("system proxy not implemented on this platform");
    Ok(())
}

#[cfg(not(windows))]
pub fn cleanup_stale_proxy(_default_port: u16) {}

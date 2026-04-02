//! Minimal system proxy for non-admin fallback mode (when TUN is unavailable).
//! Uses Windows Registry API directly — no subprocess, no WSL notifications.

use anyhow::Result;

#[cfg(windows)]
pub fn set_system_proxy(host: &str, port: u16) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinInet::InternetSetOptionW;
    use windows::Win32::System::Registry::*;

    let proxy = format!("{host}:{port}");
    tracing::info!("setting system proxy to {proxy} (fallback mode)");

    let subkey: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
        .encode_utf16().chain(std::iter::once(0)).collect();

    let mut hkey = HKEY::default();
    unsafe {
        RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), None,
            REG_SAM_FLAGS(KEY_SET_VALUE.0), &mut hkey).ok()?;

        let name_enable: Vec<u16> = "ProxyEnable".encode_utf16().chain(std::iter::once(0)).collect();
        let val: u32 = 1;
        RegSetValueExW(hkey, PCWSTR(name_enable.as_ptr()), None, REG_DWORD,
            Some(&val.to_le_bytes())).ok()?;

        let name_server: Vec<u16> = "ProxyServer".encode_utf16().chain(std::iter::once(0)).collect();
        let proxy_w: Vec<u16> = proxy.encode_utf16().chain(std::iter::once(0)).collect();
        let proxy_bytes = std::slice::from_raw_parts(proxy_w.as_ptr() as *const u8, proxy_w.len() * 2);
        RegSetValueExW(hkey, PCWSTR(name_server.as_ptr()), None, REG_SZ,
            Some(proxy_bytes)).ok()?;

        RegCloseKey(hkey);
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

    let subkey: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
        .encode_utf16().chain(std::iter::once(0)).collect();

    let mut hkey = HKEY::default();
    unsafe {
        RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), None,
            REG_SAM_FLAGS(KEY_SET_VALUE.0), &mut hkey).ok()?;

        let name_enable: Vec<u16> = "ProxyEnable".encode_utf16().chain(std::iter::once(0)).collect();
        let val: u32 = 0;
        RegSetValueExW(hkey, PCWSTR(name_enable.as_ptr()), None, REG_DWORD,
            Some(&val.to_le_bytes())).ok()?;

        RegCloseKey(hkey);
        InternetSetOptionW(None, 39, None, 0).ok();
        InternetSetOptionW(None, 37, None, 0).ok();
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_system_proxy(_host: &str, _port: u16) -> Result<()> { Ok(()) }

#[cfg(not(windows))]
pub fn unset_system_proxy() -> Result<()> { Ok(()) }

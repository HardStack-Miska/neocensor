use anyhow::{bail, Context, Result};

/// Run a reg.exe command and check exit code.
#[cfg(windows)]
fn run_reg(args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("reg")
        .args(args)
        .output()
        .context("failed to execute reg.exe")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("reg.exe failed: {}", stderr.trim());
    }
    Ok(())
}

/// Validate that host is a safe hostname/IP (no shell injection).
fn validate_proxy_host(host: &str) -> Result<()> {
    if host.is_empty() {
        bail!("proxy host is empty");
    }
    // Only allow alphanumeric, dots, dashes, colons (IPv6), brackets
    if !host.chars().all(|c| c.is_ascii_alphanumeric() || ".:-[]".contains(c)) {
        bail!("proxy host contains invalid characters: {host}");
    }
    Ok(())
}

/// Check if stale proxy settings remain from a previous crashed session.
/// If ProxyEnable=1 and ProxyServer points to our default address, clean up.
#[cfg(windows)]
pub fn cleanup_stale_proxy(default_port: u16) {
    let reg_key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    // Read ProxyEnable
    let enable_output = std::process::Command::new("reg")
        .args(["query", reg_key, "/v", "ProxyEnable"])
        .output();
    let enabled = match enable_output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).contains("0x1"),
        Err(_) => false,
    };
    if !enabled {
        return;
    }

    // Read ProxyServer
    let server_output = std::process::Command::new("reg")
        .args(["query", reg_key, "/v", "ProxyServer"])
        .output();
    let is_ours = match server_output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains(&format!("127.0.0.1:{default_port}"))
        }
        Err(_) => false,
    };

    if is_ours {
        tracing::warn!("detected stale proxy settings from previous session, cleaning up");
        if let Err(e) = unset_system_proxy() {
            tracing::error!("failed to clean stale proxy: {e}");
        }
    }
}

#[cfg(not(windows))]
pub fn cleanup_stale_proxy(_default_port: u16) {}

/// Set Windows system proxy using both ProxyServer and PAC.
/// - ProxyServer: used by most apps (Chrome, Brave, Edge, etc.)
/// - AutoConfigURL (PAC): provides DIRECT fallback for WFP Direct mode
#[cfg(windows)]
pub fn set_system_proxy_with_pac(host: &str, port: u16, pac_port: u16) -> Result<()> {
    validate_proxy_host(host)?;
    let proxy = format!("{host}:{port}");
    let pac_url = format!("http://127.0.0.1:{pac_port}/proxy.pac");
    tracing::info!("setting system proxy to {proxy} + PAC {pac_url}");

    let reg_key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    let override_str = crate::utils::proxy_override_string();

    run_reg(&["add", reg_key, "/v", "ProxyEnable", "/t", "REG_DWORD", "/d", "1", "/f"])?;
    run_reg(&["add", reg_key, "/v", "ProxyServer", "/t", "REG_SZ", "/d", &proxy, "/f"])?;
    run_reg(&["add", reg_key, "/v", "ProxyOverride", "/t", "REG_SZ", "/d", &override_str, "/f"])?;
    run_reg(&["add", reg_key, "/v", "AutoConfigURL", "/t", "REG_SZ", "/d", &pac_url, "/f"])?;

    notify_proxy_change();
    tracing::info!("system proxy set to {proxy} (+ PAC fallback)");
    Ok(())
}

/// Set direct proxy only (without PAC).
#[cfg(windows)]
pub fn set_system_proxy(host: &str, port: u16) -> Result<()> {
    validate_proxy_host(host)?;
    let proxy = format!("{host}:{port}");
    tracing::info!("setting system proxy to {proxy}");

    let reg_key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    let override_str = crate::utils::proxy_override_string();

    run_reg(&["add", reg_key, "/v", "ProxyEnable", "/t", "REG_DWORD", "/d", "1", "/f"])?;
    run_reg(&["add", reg_key, "/v", "ProxyServer", "/t", "REG_SZ", "/d", &proxy, "/f"])?;
    run_reg(&["add", reg_key, "/v", "ProxyOverride", "/t", "REG_SZ", "/d", &override_str, "/f"])?;

    notify_proxy_change();
    tracing::info!("system proxy set to {proxy}");
    Ok(())
}

/// Remove Windows system proxy settings (both PAC and manual).
#[cfg(windows)]
pub fn unset_system_proxy() -> Result<()> {
    tracing::info!("removing system proxy");

    let reg_key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    run_reg(&["add", reg_key, "/v", "ProxyEnable", "/t", "REG_DWORD", "/d", "0", "/f"])?;
    // AutoConfigURL deletion may fail if key doesn't exist — that's OK
    let _ = run_reg(&["delete", reg_key, "/v", "AutoConfigURL", "/f"]);

    notify_proxy_change();
    tracing::info!("system proxy removed");
    Ok(())
}

#[cfg(windows)]
fn notify_proxy_change() {
    let _ = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            r#"Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class WinInet { [DllImport("wininet.dll", SetLastError=true)] public static extern bool InternetSetOption(IntPtr hInternet, int dwOption, IntPtr lpBuffer, int dwBufferLength); public static void Refresh() { InternetSetOption(IntPtr.Zero, 39, IntPtr.Zero, 0); InternetSetOption(IntPtr.Zero, 37, IntPtr.Zero, 0); } }'; [WinInet]::Refresh()"#,
        ])
        .output()
        .ok();
}

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

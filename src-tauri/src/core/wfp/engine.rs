use anyhow::{anyhow, Result};

#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;

/// Newtype wrapper around HANDLE to implement Send + Sync.
/// WFP engine handles are safe to use from any thread — the WFP API
/// is fully thread-safe and serializes calls internally.
#[cfg(windows)]
struct WfpHandle(HANDLE);

#[cfg(windows)]
unsafe impl Send for WfpHandle {}
#[cfg(windows)]
unsafe impl Sync for WfpHandle {}

/// RAII wrapper around a WFP engine session.
/// Uses `FWPM_SESSION_FLAG_DYNAMIC` so all filters are auto-removed
/// when the handle is closed or the process exits.
#[cfg(windows)]
pub struct WfpEngine {
    handle: WfpHandle,
    sublayer_key: windows::core::GUID,
}

#[cfg(windows)]
fn win32_check(code: u32, msg: &str) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(anyhow!("{msg}: error code {code:#X}"))
    }
}

#[cfg(windows)]
impl WfpEngine {
    /// Open a new WFP engine session with dynamic filters.
    pub fn open() -> Result<Self> {
        use windows::Win32::NetworkManagement::WindowsFilteringPlatform::*;

        let sublayer_key = windows::core::GUID::new()?;

        unsafe {
            let mut session = FWPM_SESSION0::default();
            session.flags = FWPM_SESSION_FLAG_DYNAMIC;

            let mut handle = HANDLE::default();
            let ret = FwpmEngineOpen0(
                None,
                0xFFFFFFFF, // RPC_C_AUTHN_DEFAULT
                None,
                Some(&session),
                &mut handle,
            );
            win32_check(ret, "FwpmEngineOpen0")?;

            // Create our sublayer to group all NeoCensor filters
            let mut sublayer_name = wide_str("NeoCensor Per-App Routing");
            let mut sublayer_desc = wide_str("NeoCensor per-process traffic routing rules");

            let sublayer = FWPM_SUBLAYER0 {
                subLayerKey: sublayer_key,
                displayData: FWPM_DISPLAY_DATA0 {
                    name: windows::core::PWSTR(sublayer_name.as_mut_ptr()),
                    description: windows::core::PWSTR(sublayer_desc.as_mut_ptr()),
                },
                weight: 0x0F,
                ..Default::default()
            };

            let ret = FwpmSubLayerAdd0(handle, &sublayer, None);
            win32_check(ret, "FwpmSubLayerAdd0")?;

            tracing::info!("WFP engine opened, sublayer created");

            Ok(Self {
                handle: WfpHandle(handle),
                sublayer_key,
            })
        }
    }

    pub fn handle(&self) -> HANDLE {
        self.handle.0
    }

    pub fn sublayer_key(&self) -> &windows::core::GUID {
        &self.sublayer_key
    }

    /// Add a filter and return its runtime ID.
    pub fn add_filter(
        &self,
        filter: &windows::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_FILTER0,
    ) -> Result<u64> {
        use windows::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmFilterAdd0;

        unsafe {
            let mut filter_id = 0u64;
            let ret = FwpmFilterAdd0(self.handle.0, filter, None, Some(&mut filter_id));
            win32_check(ret, "FwpmFilterAdd0")?;
            Ok(filter_id)
        }
    }

    /// Remove a filter by its runtime ID.
    pub fn remove_filter(&self, filter_id: u64) -> Result<()> {
        use windows::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmFilterDeleteById0;

        unsafe {
            let ret = FwpmFilterDeleteById0(self.handle.0, filter_id);
            win32_check(ret, "FwpmFilterDeleteById0")?;
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for WfpEngine {
    fn drop(&mut self) {
        use windows::Win32::NetworkManagement::WindowsFilteringPlatform::{
            FwpmEngineClose0, FwpmSubLayerDeleteByKey0,
        };

        unsafe {
            let _ = FwpmSubLayerDeleteByKey0(self.handle.0, &self.sublayer_key);
            let _ = FwpmEngineClose0(self.handle.0);
        }
        tracing::info!("WFP engine closed, all filters removed");
    }
}

/// Helper: create a null-terminated wide string.
#[cfg(windows)]
fn wide_str(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// Stub for non-Windows
#[cfg(not(windows))]
pub struct WfpEngine;

#[cfg(not(windows))]
impl WfpEngine {
    pub fn open() -> Result<Self> {
        anyhow::bail!("WFP not available on this platform")
    }
}

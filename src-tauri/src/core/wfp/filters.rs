use anyhow::Result;

use super::app_id::AppIdBlob;
use super::engine::WfpEngine;

/// Create a BLOCK filter for a specific application on both IPv4 and IPv6.
/// Returns the filter IDs (v4, v6).
#[cfg(windows)]
pub fn add_block_filter(engine: &WfpEngine, app_id: &AppIdBlob, app_name: &str) -> Result<(u64, u64)> {
    let v4 = add_filter_for_app(engine, app_id, app_name, FilterAction::Block, IpVersion::V4)?;
    let v6 = add_filter_for_app(engine, app_id, app_name, FilterAction::Block, IpVersion::V6)?;
    tracing::info!("WFP BLOCK filter added for {app_name} (v4={v4}, v6={v6})");
    Ok((v4, v6))
}

/// Create a DIRECT filter: block app's connections to the proxy port only.
/// Combined with PAC fallback (PROXY host:port; DIRECT), this makes the app
/// fall back to direct connections when the proxy is unreachable via WFP block.
#[cfg(windows)]
pub fn add_direct_filter(engine: &WfpEngine, app_id: &AppIdBlob, app_name: &str, proxy_port: u16) -> Result<(u64, u64)> {
    let v4 = add_block_proxy_port(engine, app_id, app_name, proxy_port, IpVersion::V4)?;
    let v6 = add_block_proxy_port(engine, app_id, app_name, proxy_port, IpVersion::V6)?;
    tracing::info!("WFP DIRECT filter added for {app_name} (block proxy port {proxy_port}, v4={v4}, v6={v6})");
    Ok((v4, v6))
}

/// Add a default BLOCK filter (no app condition — blocks ALL unmatched traffic).
#[cfg(windows)]
pub fn add_default_block(engine: &WfpEngine) -> Result<(u64, u64)> {
    let v4 = add_default_filter(engine, FilterAction::Block, IpVersion::V4)?;
    let v6 = add_default_filter(engine, FilterAction::Block, IpVersion::V6)?;
    tracing::info!("WFP default BLOCK filter added (v4={v4}, v6={v6})");
    Ok((v4, v6))
}

#[cfg(windows)]
enum FilterAction {
    Block,
    Permit,
}

#[cfg(windows)]
enum IpVersion {
    V4,
    V6,
}

#[cfg(windows)]
fn add_filter_for_app(
    engine: &WfpEngine,
    app_id: &AppIdBlob,
    app_name: &str,
    action: FilterAction,
    ip_version: IpVersion,
) -> Result<u64> {
    use windows::Win32::NetworkManagement::WindowsFilteringPlatform::*;

    let layer_key = match ip_version {
        IpVersion::V4 => FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        IpVersion::V6 => FWPM_LAYER_ALE_AUTH_CONNECT_V6,
    };

    let action_type = match action {
        FilterAction::Block => FWP_ACTION_BLOCK,
        FilterAction::Permit => FWP_ACTION_PERMIT,
    };

    let action_label = match action {
        FilterAction::Block => "block",
        FilterAction::Permit => "permit",
    };

    let ip_label = match ip_version {
        IpVersion::V4 => "v4",
        IpVersion::V6 => "v6",
    };

    let filter_name = format!("NeoCensor {action_label} {app_name} {ip_label}");
    let mut filter_name_wide = wide_str(&filter_name);

    tracing::debug!(
        "creating WFP filter: name={}, layer={}, app_blob_size={} bytes",
        filter_name,
        ip_label,
        app_id.data.len()
    );

    // Create the app ID blob for the condition
    let mut app_blob_data = app_id.data.clone();
    let app_blob = FWP_BYTE_BLOB {
        size: app_blob_data.len() as u32,
        data: app_blob_data.as_mut_ptr(),
    };

    // Build condition: match on Application ID
    let mut condition_value = FWP_CONDITION_VALUE0::default();
    condition_value.r#type = FWP_BYTE_BLOB_TYPE;
    unsafe {
        condition_value.Anonymous.byteBlob = &app_blob as *const _ as *mut _;
    }

    let condition = FWPM_FILTER_CONDITION0 {
        fieldKey: FWPM_CONDITION_ALE_APP_ID,
        matchType: FWP_MATCH_EQUAL,
        conditionValue: condition_value,
    };

    let filter = FWPM_FILTER0 {
        displayData: FWPM_DISPLAY_DATA0 {
            name: windows::core::PWSTR(filter_name_wide.as_mut_ptr()),
            ..Default::default()
        },
        layerKey: layer_key,
        subLayerKey: *engine.sublayer_key(),
        weight: fwp_weight(10),
        numFilterConditions: 1,
        filterCondition: &condition as *const _ as *mut _,
        action: FWPM_ACTION0 {
            r#type: action_type,
            ..Default::default()
        },
        ..Default::default()
    };

    engine.add_filter(&filter)
}

/// Block a specific app's connections to a specific destination port (proxy port).
/// Uses two conditions: app_id + destination port.
#[cfg(windows)]
fn add_block_proxy_port(
    engine: &WfpEngine,
    app_id: &AppIdBlob,
    app_name: &str,
    proxy_port: u16,
    ip_version: IpVersion,
) -> Result<u64> {
    use windows::Win32::NetworkManagement::WindowsFilteringPlatform::*;

    let layer_key = match ip_version {
        IpVersion::V4 => FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        IpVersion::V6 => FWPM_LAYER_ALE_AUTH_CONNECT_V6,
    };

    let ip_label = match ip_version {
        IpVersion::V4 => "v4",
        IpVersion::V6 => "v6",
    };

    let filter_name = format!("NeoCensor direct {app_name} block-proxy-port {ip_label}");
    let mut filter_name_wide = wide_str(&filter_name);

    tracing::debug!(
        "creating WFP direct filter: name={}, layer={}, proxy_port={}, app_blob_size={} bytes",
        filter_name,
        ip_label,
        proxy_port,
        app_id.data.len()
    );

    // Condition 1: match on Application ID
    let mut app_blob_data = app_id.data.clone();
    let app_blob = FWP_BYTE_BLOB {
        size: app_blob_data.len() as u32,
        data: app_blob_data.as_mut_ptr(),
    };

    let mut app_condition_value = FWP_CONDITION_VALUE0::default();
    app_condition_value.r#type = FWP_BYTE_BLOB_TYPE;
    unsafe {
        app_condition_value.Anonymous.byteBlob = &app_blob as *const _ as *mut _;
    }

    // Condition 2: match on destination port == proxy_port
    let mut port_condition_value = FWP_CONDITION_VALUE0::default();
    port_condition_value.r#type = FWP_UINT16;
    unsafe {
        port_condition_value.Anonymous.uint16 = proxy_port;
    }

    let conditions = [
        FWPM_FILTER_CONDITION0 {
            fieldKey: FWPM_CONDITION_ALE_APP_ID,
            matchType: FWP_MATCH_EQUAL,
            conditionValue: app_condition_value,
        },
        FWPM_FILTER_CONDITION0 {
            fieldKey: FWPM_CONDITION_IP_REMOTE_PORT,
            matchType: FWP_MATCH_EQUAL,
            conditionValue: port_condition_value,
        },
    ];

    let filter = FWPM_FILTER0 {
        displayData: FWPM_DISPLAY_DATA0 {
            name: windows::core::PWSTR(filter_name_wide.as_mut_ptr()),
            ..Default::default()
        },
        layerKey: layer_key,
        subLayerKey: *engine.sublayer_key(),
        weight: fwp_weight(12), // Higher than regular block (10)
        numFilterConditions: 2,
        filterCondition: conditions.as_ptr() as *mut _,
        action: FWPM_ACTION0 {
            r#type: FWP_ACTION_BLOCK,
            ..Default::default()
        },
        ..Default::default()
    };

    engine.add_filter(&filter)
}

#[cfg(windows)]
fn add_default_filter(
    engine: &WfpEngine,
    action: FilterAction,
    ip_version: IpVersion,
) -> Result<u64> {
    use windows::Win32::NetworkManagement::WindowsFilteringPlatform::*;

    let layer_key = match ip_version {
        IpVersion::V4 => FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        IpVersion::V6 => FWPM_LAYER_ALE_AUTH_CONNECT_V6,
    };

    let action_type = match action {
        FilterAction::Block => FWP_ACTION_BLOCK,
        FilterAction::Permit => FWP_ACTION_PERMIT,
    };

    let mut filter_name_wide = wide_str("NeoCensor default rule");

    let filter = FWPM_FILTER0 {
        displayData: FWPM_DISPLAY_DATA0 {
            name: windows::core::PWSTR(filter_name_wide.as_mut_ptr()),
            ..Default::default()
        },
        layerKey: layer_key,
        subLayerKey: *engine.sublayer_key(),
        weight: fwp_weight(1),
        numFilterConditions: 0,
        filterCondition: std::ptr::null_mut(),
        action: FWPM_ACTION0 {
            r#type: action_type,
            ..Default::default()
        },
        ..Default::default()
    };

    engine.add_filter(&filter)
}

/// Create a WFP weight value. Higher = higher priority.
#[cfg(windows)]
fn fwp_weight(level: u8) -> windows::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_VALUE0 {
    use windows::Win32::NetworkManagement::WindowsFilteringPlatform::*;

    let mut weight = FWP_VALUE0::default();
    weight.r#type = FWP_UINT8;
    unsafe {
        weight.Anonymous.uint8 = level;
    }
    weight
}

#[cfg(windows)]
fn wide_str(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// Stubs for non-Windows
#[cfg(not(windows))]
pub fn add_block_filter(_engine: &super::engine::WfpEngine, _app_id: &AppIdBlob, _app_name: &str) -> Result<(u64, u64)> {
    anyhow::bail!("WFP not available")
}

#[cfg(not(windows))]
pub fn add_direct_filter(_engine: &super::engine::WfpEngine, _app_id: &AppIdBlob, _app_name: &str, _proxy_port: u16) -> Result<(u64, u64)> {
    anyhow::bail!("WFP not available")
}

#[cfg(not(windows))]
pub fn add_default_block(_engine: &super::engine::WfpEngine) -> Result<(u64, u64)> {
    anyhow::bail!("WFP not available")
}

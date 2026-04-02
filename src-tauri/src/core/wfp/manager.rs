use std::collections::HashMap;

use anyhow::Result;

use crate::models::{AppRoute, RouteMode};

use super::app_id;
use super::elevation;
use super::engine::WfpEngine;
use super::filters;

/// Info about a temporary block filter that needs to be removed after delay.
pub struct ResetNeeded {
    pub process_name: String,
    pub temp_filter_ids: (u64, u64),
}

/// High-level manager for WFP per-process routing rules.
///
/// Lifecycle:
/// - `start()` on VPN connect — opens WFP engine
/// - `apply_rules()` — syncs WFP filters with current routing rules
/// - `stop()` on VPN disconnect — closes engine (auto-removes all filters)
pub struct WfpManager {
    #[cfg(windows)]
    engine: Option<WfpEngine>,
    /// Map of process_name -> (v4_filter_id, v6_filter_id)
    active_filters: HashMap<String, (u64, u64)>,
    /// Default block filter IDs (if active)
    default_block: Option<(u64, u64)>,
}

impl WfpManager {
    pub fn new() -> Self {
        Self {
            #[cfg(windows)]
            engine: None,
            active_filters: HashMap::new(),
            default_block: None,
        }
    }

    /// Open WFP engine. Requires admin rights.
    pub fn start(&mut self) -> Result<()> {
        if !elevation::is_admin() {
            tracing::warn!("WFP requires admin rights, per-process routing disabled");
            return Ok(());
        }

        #[cfg(windows)]
        {
            let engine = WfpEngine::open()?;
            self.engine = Some(engine);
            tracing::info!("WFP manager started");
        }

        Ok(())
    }

    /// Close WFP engine and remove all filters.
    pub fn stop(&mut self) {
        self.active_filters.clear();
        self.default_block = None;

        #[cfg(windows)]
        {
            self.engine = None; // Drop closes engine + removes sublayer + all filters
        }

        tracing::info!("WFP manager stopped");
    }

    /// Check if WFP engine is active.
    pub fn is_active(&self) -> bool {
        #[cfg(windows)]
        {
            self.engine.is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    /// Apply routing rules. Removes old filters and creates new ones.
    /// `routes` — per-app rules.
    /// `default_mode` — what to do with unlisted apps.
    /// `process_monitor` — for resolving process names to exe paths.
    /// Apply rules with TCP connection reset for mode changes.
    /// `force_reset` = true adds temporary BLOCK filters to kill existing connections.
    pub fn apply_rules_with_reset(
        &mut self,
        routes: &[AppRoute],
        default_mode: RouteMode,
        proxy_port: u16,
        exe_map: &std::collections::HashMap<String, String>,
        force_reset: bool,
    ) -> Result<Vec<ResetNeeded>> {
        let resets = if force_reset {
            self.apply_temporary_blocks(routes, exe_map)?
        } else {
            vec![]
        };
        self.apply_rules_inner(routes, default_mode, proxy_port, exe_map)?;
        Ok(resets)
    }

    /// Apply rules without reset. Returns list of (process_name, exe_path) newly discovered.
    pub fn apply_rules(
        &mut self,
        routes: &[AppRoute],
        default_mode: RouteMode,
        proxy_port: u16,
        exe_map: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        self.apply_rules_inner(routes, default_mode, proxy_port, exe_map)
    }

    /// Add temporary BLOCK ALL filters for apps that need connection reset.
    /// Returns list of apps that had temporary blocks applied.
    #[cfg(windows)]
    fn apply_temporary_blocks(
        &mut self,
        routes: &[AppRoute],
        exe_map: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<ResetNeeded>> {
        let engine = match &self.engine {
            Some(e) => e,
            None => return Ok(vec![]),
        };

        let mut resets = vec![];

        for route in routes {
            let exe_path = route.exe_path.clone().or_else(|| {
                app_id::resolve_exe_path_from_map(&route.process_name, exe_map)
            });

            let exe_path = match exe_path {
                Some(p) if !p.is_empty() => p,
                _ => continue,
            };

            let wfp_app_id = match app_id::get_app_id(&exe_path) {
                Ok(id) => id,
                Err(_) => continue,
            };

            // Add temporary BLOCK ALL to kill existing TCP connections
            match filters::add_block_filter(engine, &wfp_app_id, &route.process_name) {
                Ok(ids) => {
                    tracing::info!(
                        "temporary BLOCK added for {} to reset TCP connections",
                        route.process_name
                    );
                    resets.push(ResetNeeded {
                        process_name: route.process_name.clone(),
                        temp_filter_ids: ids,
                    });
                }
                Err(e) => {
                    tracing::warn!("failed to add temp block for {}: {e}", route.process_name);
                }
            }
        }

        Ok(resets)
    }

    #[cfg(not(windows))]
    fn apply_temporary_blocks(
        &mut self,
        _routes: &[AppRoute],
        _exe_map: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<ResetNeeded>> {
        Ok(vec![])
    }

    /// Remove existing filters and add temporary BLOCK ALL for all resolvable apps.
    /// Call this, wait, then call remove_temp_blocks + apply_rules.
    pub fn apply_temporary_blocks_only(
        &mut self,
        routes: &[AppRoute],
        exe_map: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<ResetNeeded>> {
        #[cfg(windows)]
        {
            let engine = match &self.engine {
                Some(e) => e,
                None => return Ok(vec![]),
            };

            // Remove all existing filters first
            for (name, (v4, v6)) in self.active_filters.drain() {
                engine.remove_filter(v4).ok();
                engine.remove_filter(v6).ok();
                tracing::debug!("removed existing filter for {name}");
            }
            if let Some((v4, v6)) = self.default_block.take() {
                engine.remove_filter(v4).ok();
                engine.remove_filter(v6).ok();
            }

            // Add temporary blocks
            self.apply_temporary_blocks(routes, exe_map)
        }
        #[cfg(not(windows))]
        {
            let _ = (routes, exe_map);
            Ok(vec![])
        }
    }

    /// Remove temporary block filters after delay.
    pub fn remove_temp_blocks(&self, resets: &[ResetNeeded]) {
        #[cfg(windows)]
        if let Some(engine) = &self.engine {
            for reset in resets {
                engine.remove_filter(reset.temp_filter_ids.0).ok();
                engine.remove_filter(reset.temp_filter_ids.1).ok();
                tracing::debug!("removed temp block for {}", reset.process_name);
            }
        }
    }

    fn apply_rules_inner(
        &mut self,
        routes: &[AppRoute],
        default_mode: RouteMode,
        proxy_port: u16,
        exe_map: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        #[cfg(windows)]
        {
            let engine = match &self.engine {
                Some(e) => e,
                None => {
                    tracing::debug!("WFP engine not active, skipping rule application");
                    return Ok(());
                }
            };

            // Remove all existing filters
            for (name, (v4, v6)) in self.active_filters.drain() {
                if let Err(e) = engine.remove_filter(v4) {
                    tracing::warn!("failed to remove v4 filter for {name}: {e}");
                }
                if let Err(e) = engine.remove_filter(v6) {
                    tracing::warn!("failed to remove v6 filter for {name}: {e}");
                }
            }

            if let Some((v4, v6)) = self.default_block.take() {
                engine.remove_filter(v4).ok();
                engine.remove_filter(v6).ok();
            }

            // Apply per-app rules
            for route in routes {
                tracing::debug!(
                    "processing route: process={}, mode={:?}, stored_exe_path={:?}",
                    route.process_name,
                    route.mode,
                    route.exe_path
                );

                // Resolve exe path: prefer stored path, fallback to pre-built exe map
                let exe_path = route.exe_path.clone().or_else(|| {
                    app_id::resolve_exe_path_from_map(&route.process_name, exe_map)
                });

                let exe_path = match exe_path {
                    Some(p) if !p.is_empty() => {
                        tracing::debug!(
                            "resolved exe_path for {}: {}",
                            route.process_name,
                            p
                        );
                        p
                    }
                    _ => {
                        tracing::info!(
                            "cannot resolve exe path for {}, skipping WFP filter",
                            route.process_name
                        );
                        continue;
                    }
                };

                let wfp_app_id = match app_id::get_app_id(&exe_path) {
                    Ok(id) => {
                        tracing::debug!(
                            "WFP app_id obtained for {} ({}): blob_size={} bytes",
                            route.process_name,
                            exe_path,
                            id.data.len()
                        );
                        id
                    }
                    Err(e) => {
                        tracing::warn!(
                            "failed to get WFP app ID for {} ({}): {e}",
                            route.process_name,
                            exe_path
                        );
                        continue;
                    }
                };

                let result = match route.mode {
                    RouteMode::Block => {
                        tracing::info!(
                            "adding BLOCK filter for {} ({})",
                            route.process_name,
                            exe_path
                        );
                        filters::add_block_filter(engine, &wfp_app_id, &route.process_name)
                    }
                    RouteMode::Direct => {
                        tracing::info!(
                            "adding DIRECT filter for {} ({}) — blocking proxy port {}",
                            route.process_name,
                            exe_path,
                            proxy_port
                        );
                        // Block app's connections to proxy port only.
                        // Combined with PAC fallback (PROXY; DIRECT), app falls back to direct.
                        filters::add_direct_filter(engine, &wfp_app_id, &route.process_name, proxy_port)
                    }
                    RouteMode::Proxy | RouteMode::Auto => {
                        // PROXY/AUTO apps use system proxy (PAC), no WFP filter needed.
                        tracing::debug!(
                            "{} set to {:?} — using system proxy (no WFP filter)",
                            route.process_name,
                            route.mode
                        );
                        continue;
                    }
                };

                match result {
                    Ok(ids) => {
                        self.active_filters.insert(route.process_name.clone(), ids);
                    }
                    Err(e) => {
                        tracing::error!(
                            "failed to add WFP filter for {}: {e}",
                            route.process_name
                        );
                    }
                }
            }

            // Default mode
            if default_mode == RouteMode::Block {
                match filters::add_default_block(engine) {
                    Ok(ids) => {
                        self.default_block = Some(ids);
                    }
                    Err(e) => {
                        tracing::error!("failed to add default BLOCK filter: {e}");
                    }
                }
            }

            tracing::info!(
                "WFP rules applied: {} per-app filters, default={:?}",
                self.active_filters.len(),
                default_mode
            );
        }

        Ok(())
    }

    /// Get list of active filter names.
    pub fn active_filter_names(&self) -> Vec<String> {
        self.active_filters.keys().cloned().collect()
    }
}

/// Resolve exe_path for a process_name from the process monitor.
pub fn resolve_exe_for_route(
    process_name: &str,
    monitor: &mut crate::core::process_monitor::ProcessMonitor,
) -> Option<String> {
    app_id::resolve_exe_path(process_name, monitor)
}

import { invoke } from '@tauri-apps/api/core';
import type {
  AppRoute,
  AppState,
  Profile,
  RouteMode,
  RunningProcess,
  ServerConfig,
  ServerEntry,
  Settings,
  Subscription,
} from './types';

// Server commands
export const getServers = () => invoke<ServerEntry[]>('get_servers');

export const addServer = (uri: string) =>
  invoke<ServerEntry>('add_server', { uri });

export const removeServer = (serverId: string) =>
  invoke<void>('remove_server', { serverId });

export const parseVless = (uri: string) =>
  invoke<ServerConfig>('parse_vless', { uri });

export const exportVlessUri = (serverId: string) =>
  invoke<string>('export_vless_uri', { serverId });

export const pingServer = (serverId: string) =>
  invoke<number>('ping_server', { serverId });

export const pingAllServers = () =>
  invoke<[string, number | null][]>('ping_all_servers');

export const toggleFavorite = (serverId: string) =>
  invoke<boolean>('toggle_favorite', { serverId });

// Connection commands
export const connectToServer = (serverId: string) =>
  invoke<void>('connect', { serverId });

export const disconnectFromServer = () => invoke<void>('disconnect');

export const getConnectionStatus = () =>
  invoke<AppState>('get_connection_status');

// Routing commands
export const getAppRoutes = () => invoke<AppRoute[]>('get_app_routes');

export const setAppRoute = (
  processName: string,
  displayName: string,
  mode: RouteMode,
) => invoke<void>('set_app_route', { processName, displayName, mode });

export const removeAppRoute = (processName: string) =>
  invoke<void>('remove_app_route', { processName });

export const getProfiles = () => invoke<Profile[]>('get_profiles');

export const setActiveProfile = (profileId: string) =>
  invoke<void>('set_active_profile', { profileId });

export const getSettings = () => invoke<Settings>('get_settings');

export const updateSettings = (newSettings: Settings) =>
  invoke<void>('update_settings', { newSettings });

// Subscription commands
export const getSubscriptions = () =>
  invoke<Subscription[]>('get_subscriptions');

export const addSubscription = (url: string, name?: string) =>
  invoke<Subscription>('add_subscription', { url, name });

export const refreshSubscription = (subId: string) =>
  invoke<number>('refresh_subscription', { subId });

export const removeSubscription = (subId: string) =>
  invoke<void>('remove_subscription', { subId });

export const refreshAllSubscriptions = () =>
  invoke<number>('refresh_all_subscriptions');

// Process commands
export const getProcesses = () =>
  invoke<RunningProcess[]>('get_processes');

// Traffic types
export interface ConnectionEvent {
  id: number;
  time: string;
  host: string;
  port: number;
  route: string;
  protocol: string;
}

export interface TrafficSnapshot {
  connections: ConnectionEvent[];
  total_connections: number;
  active: boolean;
}

export const getTrafficStats = () =>
  invoke<TrafficSnapshot>('get_traffic_stats');

// Download/component commands
export interface BinaryStatus {
  singbox_installed: boolean;
}

export interface ComponentVersions {
  singbox_latest: string;
}

export const checkBinaries = () =>
  invoke<BinaryStatus>('check_binaries');

export const downloadComponents = () =>
  invoke<void>('download_components');

export const checkLatestVersions = () =>
  invoke<ComponentVersions>('check_latest_versions');

// Log commands
export const startLogStream = () => invoke<void>('start_log_stream');

export const getLogPath = () => invoke<string>('get_log_path');

// Updater
export const getVersion = () => invoke<string>('get_app_version').catch(() => '0.1.0');

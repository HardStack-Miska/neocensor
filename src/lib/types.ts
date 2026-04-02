// Models matching Rust backend types

export interface ServerConfig {
  id: string;
  name: string;
  address: string;
  port: number;
  uuid: string;
  flow: string | null;
  encryption: string;
  transport: TransportConfig;
  security: SecurityConfig;
}

export type TransportConfig =
  | { type: 'tcp' }
  | { type: 'ws'; path: string; host?: string }
  | { type: 'grpc'; service_name: string }
  | { type: 'xhttp'; path: string; host?: string; mode?: string };

export type SecurityConfig =
  | { type: 'none' }
  | { type: 'tls'; sni: string; fingerprint?: string; alpn?: string[] }
  | {
      type: 'reality';
      sni: string;
      fingerprint: string;
      public_key: string;
      short_id: string;
      spider_x?: string;
    };

export interface ServerEntry {
  config: ServerConfig;
  source: ServerSource;
  display_name: string;
  ping_ms: number | null;
  country: string | null;
  favorite: boolean;
  online: boolean;
}

export type ServerSource =
  | { Manual: null }
  | { Subscription: string };

export type RouteMode = 'proxy' | 'direct' | 'block' | 'auto';

export type AppCategory =
  | 'browser'
  | 'communication'
  | 'gaming'
  | 'streaming'
  | 'development'
  | 'system'
  | 'other';

export interface AppRoute {
  process_name: string;
  display_name: string;
  mode: RouteMode;
  icon_path: string | null;
  exe_path: string | null;
  category: AppCategory;
}

export interface Profile {
  id: string;
  name: string;
  icon: string;
  routes: AppRoute[];
  default_mode: RouteMode;
}

export interface Subscription {
  id: string;
  name: string;
  url: string;
  servers: ServerConfig[];
  last_updated: string | null;
  update_interval_secs: number;
  enabled: boolean;
}

export type ConnectionStatus =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'disconnecting'
  | 'error';

export interface AppState {
  status: ConnectionStatus;
  active_server_id: string | null;
  active_profile_id: string | null;
  kill_switch_enabled: boolean;
  uptime_secs: number;
  bytes_sent: number;
  bytes_received: number;
}

export interface RunningProcess {
  pid: number;
  name: string;
  exe_path: string;
  category: AppCategory;
}

export interface Settings {
  dns: DnsSettings;
  kill_switch: boolean;
  auto_connect: boolean;
  start_minimized: boolean;
  auto_start: boolean;
  theme: string;
  language: string;
  log_level: string;
  mixed_port: number;
  active_profile_id: string | null;
}

export interface DnsSettings {
  proxy_dns: string;
  direct_dns: string;
}

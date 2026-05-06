import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { ConnectionStatus } from '../lib/types';
import * as api from '../lib/tauri';
import { toast } from './toastStore';

export type VpnMode = 'off' | 'tun' | 'proxy_only';

interface ConnectionStore {
  status: ConnectionStatus;
  activeServerId: string | null;
  uptimeSecs: number;
  vpnMode: VpnMode;

  connect: (serverId: string) => Promise<void>;
  disconnect: () => Promise<void>;
  fetchStatus: () => Promise<void>;
  initListener: () => Promise<void>;
  destroy: () => void;
}

let _unlisten: UnlistenFn | null = null;
let _unlistenWarning: UnlistenFn | null = null;
let _unlistenError: UnlistenFn | null = null;
let _unlistenMode: UnlistenFn | null = null;
let _initialized = false;

export const useConnectionStore = create<ConnectionStore>((set) => ({
  status: 'disconnected',
  activeServerId: null,
  uptimeSecs: 0,
  vpnMode: 'off',

  connect: async (serverId) => {
    set({ status: 'connecting', activeServerId: serverId });
    try {
      await api.connectToServer(serverId);
    } catch (e) {
      set({ status: 'error' });
      toast.error(`Connection failed: ${e}`);
    }
  },

  disconnect: async () => {
    set({ status: 'disconnecting' });
    try {
      await api.disconnectFromServer();
    } catch (e) {
      set({ status: 'error' });
      toast.error(`Disconnect failed: ${e}`);
    }
  },

  fetchStatus: async () => {
    try {
      const state = await api.getConnectionStatus();
      set({
        status: state.status,
        activeServerId: state.active_server_id,
        uptimeSecs: state.uptime_secs,
      });
    } catch {
      // Silent — status polling failure is not user-actionable
    }
  },

  initListener: async () => {
    if (_initialized) return;
    _initialized = true;

    const validStatuses: ConnectionStatus[] = ['disconnected', 'connecting', 'connected', 'disconnecting', 'error'];
    _unlisten = await listen<string>('connection-status', (event) => {
      const status = event.payload as ConnectionStatus;
      if (validStatuses.includes(status)) {
        set({ status });
      } else {
        console.warn('unknown connection status from backend:', event.payload);
        set({ status: 'error' });
      }
    });

    // Warn user when VPN is in proxy-only fallback (per-app routing disabled)
    _unlistenWarning = await listen<{ mode: string; message: string }>('vpn-mode-warning', (event) => {
      if (event.payload?.message) {
        toast.warning(event.payload.message);
      }
    });

    // Surface watchdog/sing-box crash events
    _unlistenError = await listen<{ code: string; message: string }>('vpn-error', (event) => {
      if (event.payload?.message) {
        toast.error(event.payload.message);
      }
    });

    // Track active VPN mode (tun = full per-app routing, proxy_only = no per-app)
    _unlistenMode = await listen<string>('vpn-mode', (event) => {
      const mode = event.payload as VpnMode;
      if (mode === 'tun' || mode === 'proxy_only' || mode === 'off') {
        set({ vpnMode: mode });
      }
    });
  },

  destroy: () => {
    if (_unlisten) {
      _unlisten();
      _unlisten = null;
    }
    if (_unlistenWarning) {
      _unlistenWarning();
      _unlistenWarning = null;
    }
    if (_unlistenError) {
      _unlistenError();
      _unlistenError = null;
    }
    if (_unlistenMode) {
      _unlistenMode();
      _unlistenMode = null;
    }
    _initialized = false;
  },
}));

import { create } from 'zustand';
import type { ServerEntry, Subscription } from '../lib/types';
import * as api from '../lib/tauri';
import { toast } from './toastStore';

interface ServerStore {
  servers: ServerEntry[];
  subscriptions: Subscription[];
  loading: boolean;

  fetchServers: () => Promise<void>;
  addServer: (uri: string) => Promise<void>;
  removeServer: (id: string) => Promise<void>;
  pingServer: (id: string) => Promise<void>;
  pingAll: () => Promise<void>;
  toggleFavorite: (id: string) => Promise<void>;

  fetchSubscriptions: () => Promise<void>;
  addSubscription: (url: string, name?: string) => Promise<void>;
  removeSubscription: (id: string) => Promise<void>;
  refreshSubscription: (id: string) => Promise<void>;
  refreshAllSubscriptions: () => Promise<void>;
}

export const useServerStore = create<ServerStore>((set, get) => ({
  servers: [],
  subscriptions: [],
  loading: false,

  fetchServers: async () => {
    try {
      const servers = await api.getServers();
      set({ servers });
    } catch (e) {
      toast.error(`Failed to load servers: ${e}`);
    }
  },

  addServer: async (uri) => {
    set({ loading: true });
    try {
      await api.addServer(uri);
      await get().fetchServers();
      toast.success('Server added');
    } catch (e) {
      toast.error(`Failed to add server: ${e}`);
    } finally {
      set({ loading: false });
    }
  },

  removeServer: async (id) => {
    try {
      await api.removeServer(id);
      set((s) => ({ servers: s.servers.filter((srv) => srv.config.id !== id) }));
      toast.success('Server removed');
    } catch (e) {
      toast.error(`Failed to remove server: ${e}`);
    }
  },

  pingServer: async (id) => {
    try {
      const ms = await api.pingServer(id);
      set((s) => ({
        servers: s.servers.map((srv) =>
          srv.config.id === id
            ? { ...srv, ping_ms: ms, online: true }
            : srv,
        ),
      }));
    } catch {
      set((s) => ({
        servers: s.servers.map((srv) =>
          srv.config.id === id ? { ...srv, online: false } : srv,
        ),
      }));
    }
  },

  pingAll: async () => {
    try {
      const results = await api.pingAllServers();
      set((s) => ({
        servers: s.servers.map((srv) => {
          const result = results.find(([id]) => id === srv.config.id);
          if (result) {
            return {
              ...srv,
              ping_ms: result[1],
              online: result[1] !== null,
            };
          }
          return srv;
        }),
      }));
    } catch (e) {
      toast.error(`Ping failed: ${e}`);
    }
  },

  toggleFavorite: async (id) => {
    try {
      const isFav = await api.toggleFavorite(id);
      set((s) => ({
        servers: s.servers.map((srv) =>
          srv.config.id === id ? { ...srv, favorite: isFav } : srv,
        ),
      }));
    } catch (e) {
      toast.error(`Failed to toggle favorite: ${e}`);
    }
  },

  fetchSubscriptions: async () => {
    try {
      const subscriptions = await api.getSubscriptions();
      set({ subscriptions });
    } catch (e) {
      toast.error(`Failed to load subscriptions: ${e}`);
    }
  },

  addSubscription: async (url, name) => {
    set({ loading: true });
    try {
      const sub = await api.addSubscription(url, name);
      await get().fetchSubscriptions();
      await get().fetchServers();
      toast.success(`Subscription added: ${sub.name}`);
    } catch (e) {
      toast.error(`Failed to add subscription: ${e}`);
    } finally {
      set({ loading: false });
    }
  },

  removeSubscription: async (id) => {
    try {
      await api.removeSubscription(id);
      await get().fetchSubscriptions();
      await get().fetchServers();
      toast.success('Subscription removed');
    } catch (e) {
      toast.error(`Failed to remove subscription: ${e}`);
    }
  },

  refreshSubscription: async (id) => {
    try {
      const count = await api.refreshSubscription(id);
      await get().fetchSubscriptions();
      await get().fetchServers();
      toast.success(`Subscription updated: ${count} servers`);
    } catch (e) {
      toast.error(`Failed to refresh subscription: ${e}`);
    }
  },

  refreshAllSubscriptions: async () => {
    set({ loading: true });
    try {
      const total = await api.refreshAllSubscriptions();
      await get().fetchSubscriptions();
      await get().fetchServers();
      toast.success(`All subscriptions updated: ${total} servers`);
    } catch (e) {
      toast.error(`Failed to refresh subscriptions: ${e}`);
    } finally {
      set({ loading: false });
    }
  },
}));

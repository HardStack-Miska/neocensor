import { create } from 'zustand';
import type { Settings } from '../lib/types';
import * as api from '../lib/tauri';
import { toast } from './toastStore';

interface SettingsStore {
  settings: Settings;
  loaded: boolean;

  fetchSettings: () => Promise<void>;
  updateSettings: (settings: Settings) => Promise<void>;
}

const defaultSettings: Settings = {
  dns: {
    proxy_dns: 'https://8.8.8.8/dns-query',
    direct_dns: '77.88.8.8',
  },
  kill_switch: true,
  auto_connect: false,
  start_minimized: false,
  auto_start: false,
  theme: 'dark',
  language: 'ru',
  log_level: 'warn',
  mixed_port: 2080,
  active_profile_id: null,
};

export const useSettingsStore = create<SettingsStore>((set) => ({
  settings: defaultSettings,
  loaded: false,

  fetchSettings: async () => {
    try {
      const settings = await api.getSettings();
      set({ settings, loaded: true });
    } catch {
      set({ loaded: true });
    }
  },

  updateSettings: async (settings) => {
    try {
      await api.updateSettings(settings);
      set({ settings });
      toast.success('Settings saved');
    } catch (e) {
      toast.error(`Failed to save settings: ${e}`);
    }
  },
}));

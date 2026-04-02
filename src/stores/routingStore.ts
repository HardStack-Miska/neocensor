import { create } from 'zustand';
import type { AppRoute, Profile, RouteMode, RunningProcess } from '../lib/types';
import * as api from '../lib/tauri';
import { toast } from './toastStore';

interface RoutingStore {
  routes: AppRoute[];
  profiles: Profile[];
  activeProfileId: string | null;
  processes: RunningProcess[];
  loading: boolean;
  profileLoading: boolean;

  fetchRoutes: () => Promise<void>;
  setRoute: (processName: string, displayName: string, mode: RouteMode) => Promise<void>;
  removeRoute: (processName: string) => Promise<void>;

  fetchProfiles: () => Promise<void>;
  setActiveProfile: (profileId: string) => Promise<void>;

  fetchProcesses: () => Promise<void>;
}

export const useRoutingStore = create<RoutingStore>((set, get) => ({
  routes: [],
  profiles: [],
  activeProfileId: null,
  processes: [],
  loading: false,
  profileLoading: false,

  fetchRoutes: async () => {
    try {
      const routes = await api.getAppRoutes();
      const state = await api.getConnectionStatus();
      set({ routes, activeProfileId: state.active_profile_id });
    } catch (e) {
      toast.error(`Failed to load routes: ${e}`);
    }
  },

  setRoute: async (processName, displayName, mode) => {
    try {
      await api.setAppRoute(processName, displayName, mode);
      await get().fetchRoutes();
    } catch (e) {
      toast.error(`Failed to set route for ${displayName}: ${e}`);
    }
  },

  removeRoute: async (processName) => {
    try {
      await api.removeAppRoute(processName);
      set((s) => ({
        routes: s.routes.filter((r) => r.process_name !== processName),
      }));
    } catch (e) {
      toast.error(`Failed to remove route: ${e}`);
    }
  },

  fetchProfiles: async () => {
    try {
      const profiles = await api.getProfiles();
      set({ profiles });
    } catch (e) {
      toast.error(`Failed to load profiles: ${e}`);
    }
  },

  setActiveProfile: async (profileId) => {
    set({ profileLoading: true });
    try {
      await api.setActiveProfile(profileId);
      set({ activeProfileId: profileId });
      await get().fetchRoutes();
      toast.success('Profile switched');
    } catch (e) {
      toast.error(`Failed to switch profile: ${e}`);
    } finally {
      set({ profileLoading: false });
    }
  },

  fetchProcesses: async () => {
    set({ loading: true });
    try {
      const processes = await api.getProcesses();
      set({ processes });
    } catch (e) {
      toast.error(`Failed to load processes: ${e}`);
    } finally {
      set({ loading: false });
    }
  },
}));

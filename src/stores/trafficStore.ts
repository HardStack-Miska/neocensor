import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { ConnectionEvent } from '../lib/tauri';

const MAX_LOG = 200;
const MAX_WAVE_POINTS = 60;

interface TrafficStore {
  connections: ConnectionEvent[];
  totalCount: number;
  wavePoints: number[];
  lastWaveTotal: number;
  initialized: boolean;

  initListener: () => Promise<void>;
  clear: () => void;
  destroy: () => void;
}

let _unlisten: UnlistenFn | null = null;
let _intervalId: ReturnType<typeof setInterval> | null = null;

export const useTrafficStore = create<TrafficStore>((set, get) => ({
  connections: [],
  totalCount: 0,
  wavePoints: new Array(MAX_WAVE_POINTS).fill(0),
  lastWaveTotal: 0,
  initialized: false,

  initListener: async () => {
    if (get().initialized) return;
    set({ initialized: true });

    _unlisten = await listen<ConnectionEvent>('connection-event', (event) => {
      set((state) => {
        const next = [event.payload, ...state.connections];
        if (next.length > MAX_LOG) next.length = MAX_LOG;
        return {
          connections: next,
          totalCount: state.totalCount + 1,
        };
      });
    });

    _intervalId = setInterval(() => {
      const state = get();
      const delta = state.totalCount - state.lastWaveTotal;
      const nextPoints = [...state.wavePoints.slice(1), delta];
      set({
        wavePoints: nextPoints,
        lastWaveTotal: state.totalCount,
      });
    }, 2000);
  },

  clear: () => set({
    connections: [],
    totalCount: 0,
    wavePoints: new Array(MAX_WAVE_POINTS).fill(0),
    lastWaveTotal: 0,
  }),

  destroy: () => {
    if (_unlisten) {
      _unlisten();
      _unlisten = null;
    }
    if (_intervalId) {
      clearInterval(_intervalId);
      _intervalId = null;
    }
    set({
      connections: [],
      totalCount: 0,
      wavePoints: new Array(MAX_WAVE_POINTS).fill(0),
      lastWaveTotal: 0,
      initialized: false,
    });
  },
}));

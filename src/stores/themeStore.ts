import { create } from 'zustand';
import { darkTheme, lightTheme, type Theme } from '../lib/theme';

interface ThemeStore {
  dark: boolean;
  theme: Theme;
  toggle: () => void;
}

export const useThemeStore = create<ThemeStore>((set) => ({
  dark: true,
  theme: darkTheme,
  toggle: () =>
    set((s) => ({
      dark: !s.dark,
      theme: s.dark ? lightTheme : darkTheme,
    })),
}));

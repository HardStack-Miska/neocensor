import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getVersion } from '@tauri-apps/api/app';
import { Shield, Minus, Maximize2, X, Sun, Moon } from 'lucide-react';
import { useThemeStore } from '../../stores/themeStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useServerStore } from '../../stores/serverStore';
import { MONO, SANS } from '../../lib/theme';
import type { CSSProperties } from 'react';

const winBtn: CSSProperties = {
  background: 'none',
  border: 'none',
  cursor: 'pointer',
  padding: '0 14px',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  transition: 'background .1s',
};

export const Titlebar = () => {
  const { theme: T, dark, toggle } = useThemeStore();
  const status = useConnectionStore((s) => s.status);
  const activeServerId = useConnectionStore((s) => s.activeServerId);
  const servers = useServerStore((s) => s.servers);

  const [appVersion, setAppVersion] = useState('');
  const appWindow = getCurrentWindow();

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  const activeServer = servers.find((s) => s.config.id === activeServerId);
  const isOn = status === 'connected';
  const isBusy = status === 'connecting' || status === 'disconnecting';

  return (
    <div
      data-tauri-drag-region
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '0 4px 0 14px',
        height: 38,
        flexShrink: 0,
        background: T.side,
        borderBottom: `1px solid ${T.brd}`,
        fontFamily: SANS,
      }}
    >
      {/* Left: logo */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <Shield size={13} style={{ color: T.ac }} />
        <span style={{ fontSize: 12, fontWeight: 600, letterSpacing: '-0.02em', color: T.t0 }}>
          NeoCensor
        </span>
        {appVersion && (
          <span style={{ fontSize: 9, color: T.t3, fontWeight: 500, marginLeft: 2 }}>
            v{appVersion}
          </span>
        )}
      </div>

      {/* Center: status pill */}
      <div style={{ position: 'absolute', left: '50%', transform: 'translateX(-50%)' }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 7,
            padding: '4px 12px',
            borderRadius: 4,
            background: isOn ? T.okS : isBusy ? T.acS : T.bg2,
            transition: 'all .3s',
          }}
        >
          <div
            style={{
              width: 5,
              height: 5,
              borderRadius: 3,
              background: isOn ? T.ok : isBusy ? T.ac : T.t3,
              animation: isBusy ? 'br 1.3s ease infinite' : 'none',
            }}
          />
          <span
            style={{
              fontSize: 11,
              fontWeight: 500,
              color: isOn ? T.ok : isBusy ? T.ac : T.t2,
            }}
          >
            {isBusy
              ? 'Connecting…'
              : isOn
                ? activeServer?.display_name ?? 'Connected'
                : 'Disconnected'}
          </span>
          {isOn && activeServer?.ping_ms != null && (
            <span style={{ fontSize: 10, fontFamily: MONO, color: T.t2 }}>
              {activeServer.ping_ms}ms
            </span>
          )}
        </div>
      </div>

      {/* Right: window controls */}
      <div style={{ display: 'flex', alignItems: 'stretch', height: '100%', marginRight: -4 }}>
        <button onClick={toggle} style={{ ...winBtn, color: T.t2 }}>
          {dark ? <Sun size={13} /> : <Moon size={13} />}
        </button>
        <button onClick={() => appWindow.minimize()} style={{ ...winBtn, color: T.winBtn }}>
          <Minus size={14} />
        </button>
        <button onClick={() => appWindow.toggleMaximize()} style={{ ...winBtn, color: T.winBtn }}>
          <Maximize2 size={12} />
        </button>
        <button
          onClick={() => appWindow.close()}
          style={{ ...winBtn, color: T.winClose, borderRadius: '0 7px 0 0' }}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLButtonElement).style.background = '#E04040';
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLButtonElement).style.background = 'transparent';
          }}
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
};

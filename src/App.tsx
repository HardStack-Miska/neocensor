import { useState, useEffect } from 'react';
import { Shield, Activity, Settings, Terminal } from 'lucide-react';
import { Titlebar } from './components/Layout/Titlebar';
import { Sidebar } from './components/Sidebar/Sidebar';
import { RoutingPanel } from './components/Routing/RoutingPanel';
import { TrafficPanel } from './components/Traffic/TrafficPanel';
import { SettingsPanel } from './components/Settings/SettingsPanel';
import { LogsPanel } from './components/Settings/LogsPanel';
import { ToastContainer } from './components/common/Toast';
import { useThemeStore } from './stores/themeStore';
import { useConnectionStore } from './stores/connectionStore';
import { useTrafficStore } from './stores/trafficStore';
import { SANS } from './lib/theme';
import type { LucideIcon } from 'lucide-react';

type Tab = 'routing' | 'traffic' | 'logs' | 'settings';

const TABS: [Tab, string, LucideIcon][] = [
  ['routing', 'Routing', Shield],
  ['traffic', 'Traffic', Activity],
  ['logs', 'Logs', Terminal],
  ['settings', 'Settings', Settings],
];

const App = () => {
  const T = useThemeStore((s) => s.theme);
  const initConnectionListener = useConnectionStore((s) => s.initListener);
  const initTrafficListener = useTrafficStore((s) => s.initListener);
  const [tab, setTab] = useState<Tab>('routing');

  useEffect(() => {
    initConnectionListener();
    initTrafficListener();
  }, [initConnectionListener, initTrafficListener]);

  return (
    <div
      style={{
        fontFamily: SANS,
        width: '100%',
        height: '100vh',
        display: 'flex',
        flexDirection: 'column',
        background: T.bg0,
        color: T.t0,
        overflow: 'hidden',
        transition: 'background .4s, color .3s',
      }}
    >
      {/* Global styles & animations */}
      {/* Fonts loaded via index.html <link> */}
      <style>{`
        * { box-sizing: border-box; margin: 0; padding: 0; }
        ::-webkit-scrollbar { width: 3px; }
        ::-webkit-scrollbar-track { background: transparent; }
        ::-webkit-scrollbar-thumb { background: ${T.brd}; border-radius: 9px; }
        @keyframes fu { from { opacity: 0; transform: translateY(2px); } to { opacity: 1; transform: translateY(0); } }
        @keyframes si { from { opacity: 0; transform: scale(.98); } to { opacity: 1; transform: scale(1); } }
        @keyframes spin { to { transform: rotate(360deg); } }
        @keyframes br { 0%,100% { opacity: .5; } 50% { opacity: 1; } }
        body { margin: 0; padding: 0; overflow: hidden; user-select: none; }
        [data-tauri-drag-region] { -webkit-app-region: drag; }
      `}</style>

      <Titlebar />

      <div style={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
        <Sidebar />

        {/* Main content */}
        <div
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            overflow: 'hidden',
            background: T.bg0,
          }}
        >
          {/* Tabs header */}
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              padding: '10px 20px',
              borderBottom: `1px solid ${T.brd}`,
              flexShrink: 0,
            }}
          >
            <div style={{ display: 'flex', background: T.bg2, borderRadius: 6, padding: 2 }}>
              {TABS.map(([key, label, Icon]) => (
                <button
                  key={key}
                  onClick={() => setTab(key)}
                  style={{
                    padding: '6px 18px',
                    border: 'none',
                    borderRadius: 4,
                    background: tab === key ? T.bg1 : 'transparent',
                    color: tab === key ? T.t0 : T.t2,
                    boxShadow: tab === key ? T.shS : 'none',
                    fontSize: 11.5,
                    fontWeight: 500,
                    cursor: 'pointer',
                    fontFamily: SANS,
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    transition: 'all .15s',
                  }}
                >
                  <Icon size={12.5} />
                  {label}
                </button>
              ))}
            </div>
          </div>

          {/* Tab content */}
          {tab === 'routing' && <RoutingPanel />}
          {tab === 'traffic' && <TrafficPanel />}
          {tab === 'logs' && <LogsPanel />}
          {tab === 'settings' && <SettingsPanel />}
        </div>
      </div>
      <ToastContainer />
    </div>
  );
};

export default App;

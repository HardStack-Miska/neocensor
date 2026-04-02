import { useState, useMemo, useEffect, useRef, useCallback } from 'react';
import { createPortal } from 'react-dom';
import {
  Shield,
  Globe,
  Activity,
  Zap,
  ChevronDown,
  Search,
  Plus,
  X,
  Route,
  Monitor,
  Terminal,
  Gamepad2,
  Cpu,
  ShieldCheck,
  ShieldAlert,
} from 'lucide-react';
import { useThemeStore } from '../../stores/themeStore';
import { useRoutingStore } from '../../stores/routingStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { StatCard } from '../common/StatCard';
import { MONO, SANS, modeColor, modeBg } from '../../lib/theme';
import type { RouteMode, AppRoute, RunningProcess } from '../../lib/types';
import type { LucideIcon } from 'lucide-react';
import * as api from '../../lib/tauri';

const MODE_ORDER: RouteMode[] = ['proxy', 'direct', 'auto', 'block'];
const MODE_LABELS: Record<RouteMode, string> = {
  proxy: 'Proxy',
  direct: 'Direct',
  auto: 'Auto',
  block: 'Block',
};
const MODE_ICONS: Record<RouteMode, LucideIcon> = {
  proxy: Shield,
  direct: Globe,
  auto: Route,
  block: X,
};

const APP_ICONS: Record<string, LucideIcon> = {
  Discord: Terminal,
  'Google Chrome': Globe,
  Chrome: Globe,
  Telegram: Zap,
  'Dota 2': Gamepad2,
  Steam: Cpu,
  Spotify: Activity,
  'VS Code': Terminal,
  Firefox: Globe,
};

export const RoutingPanel = () => {
  const T = useThemeStore((s) => s.theme);
  const { routes, fetchRoutes, setRoute, fetchProcesses, processes } = useRoutingStore();
  const status = useConnectionStore((s) => s.status);
  const killSwitch = useSettingsStore((s) => s.settings.kill_switch);
  const [query, setQuery] = useState('');
  const [showAdd, setShowAdd] = useState(false);
  const [isAdmin, setIsAdmin] = useState<boolean | null>(null);
  const [wfpActive, setWfpActive] = useState(false);

  useEffect(() => {
    fetchRoutes();
    api.checkAdmin().then(setIsAdmin).catch(() => {});
  }, [fetchRoutes]);

  useEffect(() => {
    if (status === 'connected') {
      api.isWfpActive().then(setWfpActive).catch(() => {});
    } else {
      setWfpActive(false);
    }
  }, [status]);

  const visible = useMemo(
    () =>
      query
        ? routes.filter(
            (a) =>
              a.display_name.toLowerCase().includes(query.toLowerCase()) ||
              a.process_name.toLowerCase().includes(query.toLowerCase()),
          )
        : routes,
    [routes, query],
  );

  const [dropdown, setDropdown] = useState<{ key: string; rect: DOMRect } | null>(null);

  const openDropdown = useCallback((key: string, btnEl: HTMLButtonElement) => {
    const rect = btnEl.getBoundingClientRect();
    setDropdown((prev) => (prev?.key === key ? null : { key, rect }));
  }, []);

  const closeDropdown = useCallback(() => setDropdown(null), []);

  const pickMode = (route: AppRoute, mode: RouteMode) => {
    setRoute(route.process_name, route.display_name, mode);
    setDropdown(null);
  };

  const stats = useMemo(() => {
    const px = routes.filter((r) => r.mode === 'proxy').length;
    const dr = routes.filter((r) => r.mode === 'direct').length;
    return {
      proxied: `${px} apps`,
      direct: `${dr} apps`,
      rules: `${routes.length}`,
      killSwitch: killSwitch ? 'On' : 'Off',
    };
  }, [routes]);

  return (
    <div style={{ flex: 1, overflowY: 'auto', padding: '16px 20px', fontFamily: SANS }}>
      {/* Stats row */}
      <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
        <StatCard label="Proxied" value={stats.proxied} color={T.mp} bg={T.mpB} icon={Shield} />
        <StatCard label="Direct" value={stats.direct} color={T.t2} bg={T.bg2} icon={Globe} />
        <StatCard label="Rules" value={stats.rules} color={T.ok} bg={T.okS} icon={Activity} />
        <StatCard label="Kill Switch" value={stats.killSwitch} color={T.er} bg={T.erS} icon={Zap} />
      </div>

      {/* Search bar */}
      <div style={{ position: 'relative', marginBottom: 10 }}>
        <Search
          size={13}
          style={{ color: T.t3, position: 'absolute', left: 10, top: '50%', transform: 'translateY(-50%)' }}
        />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Filter apps…"
          style={{
            width: '100%',
            padding: '7px 10px 7px 30px',
            borderRadius: 6,
            border: `1px solid ${T.brd}`,
            background: T.input,
            color: T.t0,
            fontSize: 11.5,
            fontFamily: SANS,
            outline: 'none',
          }}
        />
      </div>

      {/* App list */}
      <div
        style={{
          background: T.bg1,
          borderRadius: 8,
          border: `1px solid ${T.brd}`,
          overflow: 'hidden',
        }}
      >
        {visible.map((app, i) => {
          const AppIcon = APP_ICONS[app.display_name] ?? Monitor;
          const ModeIcon = MODE_ICONS[app.mode];
          return (
            <div key={app.process_name} style={{ animation: `fu .18s ease ${i * 20}ms both` }}>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  padding: '10px 14px',
                  transition: 'background .1s',
                  cursor: 'default',
                }}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLDivElement).style.background = T.hover;
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLDivElement).style.background = 'transparent';
                }}
              >
                {/* App info */}
                <div style={{ display: 'flex', alignItems: 'center', gap: 11, minWidth: 180 }}>
                  <div
                    style={{
                      width: 32,
                      height: 32,
                      borderRadius: 7,
                      background: T.bg2,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      color: T.t1,
                      flexShrink: 0,
                    }}
                  >
                    <AppIcon size={15} strokeWidth={1.5} />
                  </div>
                  <div>
                    <div style={{ fontSize: 12.5, fontWeight: 600, letterSpacing: '-0.01em' }}>
                      {app.display_name}
                    </div>
                    <div
                      style={{
                        fontSize: 9.5,
                        color: T.t3,
                        fontFamily: MONO,
                        marginTop: 1.5,
                      }}
                    >
                      {app.process_name}
                    </div>
                  </div>
                </div>

                {/* Spacer */}
                <div style={{ flex: 1 }} />

                {/* Mode selector */}
                <button
                  onClick={(e) => openDropdown(app.process_name, e.currentTarget)}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 5,
                    padding: '4px 11px',
                    borderRadius: 4,
                    border: 'none',
                    background: modeBg(T, app.mode),
                    color: modeColor(T, app.mode),
                    fontSize: 10.5,
                    fontWeight: 600,
                    cursor: 'pointer',
                    fontFamily: SANS,
                    transition: 'all .12s',
                    flexShrink: 0,
                  }}
                >
                  <ModeIcon size={11} />
                  {MODE_LABELS[app.mode]}
                  <ChevronDown
                    size={9}
                    style={{
                      opacity: 0.5,
                      transform: dropdown?.key === app.process_name ? 'rotate(180deg)' : 'none',
                      transition: 'transform .15s',
                    }}
                  />
                </button>
              </div>
              {i < visible.length - 1 && (
                <div
                  style={{
                    height: 1,
                    background: T.brdSub,
                    marginLeft: 57,
                    marginRight: 14,
                  }}
                />
              )}
            </div>
          );
        })}
        {visible.length === 0 && (
          <div
            style={{
              padding: '24px 14px',
              textAlign: 'center',
              fontSize: 11,
              color: T.t3,
            }}
          >
            {query ? 'No matching apps' : 'No routing rules configured'}
          </div>
        )}
      </div>

      {/* WFP status indicator */}
      <div
        style={{
          padding: '8px 12px',
          marginTop: 8,
          borderRadius: 6,
          background: wfpActive ? T.okS : isAdmin === false ? T.erS : T.bg2,
          fontSize: 10,
          color: wfpActive ? T.ok : isAdmin === false ? T.er : T.t3,
          lineHeight: 1.5,
          display: 'flex',
          alignItems: 'center',
          gap: 6,
        }}
      >
        {wfpActive ? (
          <>
            <ShieldCheck size={12} />
            WFP per-process routing active. Block and Direct rules are enforced.
            Proxy mode uses system proxy.
          </>
        ) : isAdmin === false ? (
          <>
            <ShieldAlert size={12} />
            Run as Administrator to enable per-process routing (Block/Direct).
            Currently all traffic uses system proxy.
          </>
        ) : status === 'connected' ? (
          <>
            <Shield size={12} />
            Per-process routing active. Block and Direct rules are enforced.
          </>
        ) : (
          <>
            <Shield size={12} />
            Connect to a server to activate per-process routing rules.
          </>
        )}
      </div>

      {/* Add application */}
      <button
        onClick={() => {
          setShowAdd(true);
          fetchProcesses();
        }}
        style={{
          width: '100%',
          marginTop: 8,
          padding: '9px 0',
          border: `1px dashed ${T.brd}`,
          borderRadius: 6,
          background: 'transparent',
          color: T.t3,
          fontSize: 11,
          cursor: 'pointer',
          fontFamily: SANS,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 5,
        }}
      >
        <Plus size={12} />
        Add application
      </button>

      {/* Add process dialog */}
      {showAdd && (
        <AddProcessOverlay
          processes={processes}
          onAdd={(proc) => {
            setRoute(proc.name, proc.name.replace('.exe', ''), 'proxy');
            setShowAdd(false);
          }}
          onClose={() => setShowAdd(false)}
        />
      )}

      {/* Mode dropdown portal — rendered outside overflow:hidden container */}
      {dropdown &&
        createPortal(
          <ModeDropdown
            rect={dropdown.rect}
            currentMode={
              visible.find((r) => r.process_name === dropdown.key)?.mode ?? 'proxy'
            }
            onPick={(mode) => {
              const app = visible.find((r) => r.process_name === dropdown.key);
              if (app) pickMode(app, mode);
            }}
            onClose={closeDropdown}
          />,
          document.body,
        )}
    </div>
  );
};

/* ── Mode dropdown (rendered via portal to escape overflow:hidden) ── */

interface ModeDropdownProps {
  rect: DOMRect;
  currentMode: RouteMode;
  onPick: (mode: RouteMode) => void;
  onClose: () => void;
}

const ModeDropdown = ({ rect, currentMode, onPick, onClose }: ModeDropdownProps) => {
  const T = useThemeStore((s) => s.theme);
  const menuRef = useRef<HTMLDivElement>(null);

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [onClose]);

  // Position below the trigger button, aligned to its right edge
  const top = rect.bottom + 4;
  const right = window.innerWidth - rect.right;

  return (
    <>
      {/* Backdrop */}
      <div
        style={{ position: 'fixed', inset: 0, zIndex: 9998 }}
        onClick={onClose}
      />
      {/* Menu */}
      <div
        ref={menuRef}
        style={{
          position: 'fixed',
          top,
          right,
          zIndex: 9999,
          background: T.bg0,
          border: `1px solid ${T.brd}`,
          borderRadius: 8,
          boxShadow: T.shL,
          minWidth: 130,
          padding: 4,
          animation: 'si .1s ease',
          fontFamily: SANS,
        }}
      >
        {MODE_ORDER.map((m) => {
          const Icon = MODE_ICONS[m];
          const active = m === currentMode;
          return (
            <button
              key={m}
              onClick={() => onPick(m)}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                width: '100%',
                padding: '6px 10px',
                border: 'none',
                borderRadius: 5,
                background: active ? modeBg(T, m) : 'transparent',
                color: active ? modeColor(T, m) : T.t1,
                fontSize: 11,
                fontWeight: active ? 600 : 400,
                cursor: 'pointer',
                fontFamily: SANS,
                transition: 'background .1s',
              }}
              onMouseEnter={(e) => {
                if (!active) (e.currentTarget as HTMLButtonElement).style.background = T.hover;
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background = active
                  ? modeBg(T, m)
                  : 'transparent';
              }}
            >
              <Icon size={12} />
              {MODE_LABELS[m]}
            </button>
          );
        })}
      </div>
    </>
  );
};

/* ── Add process overlay ── */

interface AddProcessOverlayProps {
  processes: RunningProcess[];
  onAdd: (proc: RunningProcess) => void;
  onClose: () => void;
}

const CATEGORY_ICONS: Record<string, LucideIcon> = {
  browser: Globe,
  communication: Zap,
  gaming: Gamepad2,
  streaming: Activity,
  development: Terminal,
  system: Cpu,
  other: Monitor,
};

const AddProcessOverlay = ({ processes, onAdd, onClose }: AddProcessOverlayProps) => {
  const T = useThemeStore((s) => s.theme);
  const { routes } = useRoutingStore();
  const [search, setSearch] = useState('');

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [onClose]);

  const existingNames = new Set(routes.map((r) => r.process_name));

  const filtered = processes
    .filter(
      (p) =>
        p.name.toLowerCase().includes(search.toLowerCase()) ||
        p.exe_path.toLowerCase().includes(search.toLowerCase()),
    )
    .sort((a, b) => {
      // Already added last
      const aAdded = existingNames.has(a.name) ? 1 : 0;
      const bAdded = existingNames.has(b.name) ? 1 : 0;
      if (aAdded !== bAdded) return aAdded - bAdded;
      // Known categories first
      const catOrder = ['browser', 'communication', 'gaming', 'streaming', 'development', 'system', 'other'];
      return catOrder.indexOf(a.category) - catOrder.indexOf(b.category);
    });

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 50,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'rgba(0,0,0,0.5)',
      }}
      onClick={onClose}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 420,
          borderRadius: 10,
          border: `1px solid ${T.brd}`,
          background: T.bg1,
          boxShadow: T.shL,
          overflow: 'hidden',
          animation: 'si .15s ease',
        }}
      >
        <div
          style={{
            padding: '12px 14px',
            borderBottom: `1px solid ${T.brd}`,
          }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
            <div style={{ fontSize: 13, fontWeight: 600 }}>Add Application</div>
            <span style={{ fontSize: 10, color: T.t3 }}>{filtered.length} processes</span>
          </div>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              padding: '6px 8px',
              borderRadius: 5,
              border: `1px solid ${T.brd}`,
              background: T.input,
            }}
          >
            <Search size={12} style={{ color: T.t3, flexShrink: 0 }} />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search running processes…"
              autoFocus
              style={{
                flex: 1,
                border: 'none',
                background: 'transparent',
                color: T.t0,
                fontSize: 11,
                fontFamily: SANS,
                outline: 'none',
              }}
            />
          </div>
        </div>
        <div style={{ maxHeight: 320, overflowY: 'auto' }}>
          {filtered.map((proc) => {
            const alreadyAdded = existingNames.has(proc.name);
            const CatIcon = CATEGORY_ICONS[proc.category] ?? Monitor;
            return (
              <button
                key={`${proc.pid}-${proc.name}`}
                onClick={() => !alreadyAdded && onAdd(proc)}
                disabled={alreadyAdded}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 10,
                  padding: '8px 14px',
                  width: '100%',
                  border: 'none',
                  background: 'transparent',
                  cursor: alreadyAdded ? 'default' : 'pointer',
                  fontFamily: SANS,
                  textAlign: 'left',
                  transition: 'background .1s',
                  opacity: alreadyAdded ? 0.4 : 1,
                }}
                onMouseEnter={(e) => {
                  if (!alreadyAdded) (e.currentTarget as HTMLButtonElement).style.background = T.hover;
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLButtonElement).style.background = 'transparent';
                }}
              >
                <div
                  style={{
                    width: 26,
                    height: 26,
                    borderRadius: 5,
                    background: T.bg2,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    color: T.t2,
                    flexShrink: 0,
                  }}
                >
                  <CatIcon size={12} />
                </div>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 11.5, fontWeight: 500, color: T.t0 }}>{proc.name}</div>
                  <div
                    style={{
                      fontSize: 9,
                      color: T.t3,
                      fontFamily: MONO,
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                      marginTop: 1,
                    }}
                  >
                    {proc.exe_path || proc.category}
                  </div>
                </div>
                <span
                  style={{
                    fontSize: 9,
                    fontWeight: 600,
                    padding: '2px 6px',
                    borderRadius: 3,
                    color: alreadyAdded ? T.ok : T.t3,
                    background: alreadyAdded ? T.okS : T.bg2,
                    textTransform: 'capitalize',
                    flexShrink: 0,
                  }}
                >
                  {alreadyAdded ? 'added' : proc.category}
                </span>
              </button>
            );
          })}
          {filtered.length === 0 && (
            <div style={{ padding: '16px', textAlign: 'center', fontSize: 11, color: T.t3 }}>
              No processes found
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

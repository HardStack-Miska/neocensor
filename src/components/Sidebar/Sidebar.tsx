import { useState, useEffect } from 'react';
import {
  Wifi,
  WifiOff,
  Loader2,
  Plus,
  X,
  Copy,
  Check,
  Shield,
  Globe,
  Radio,
  Gamepad2,
  Briefcase,
  Route,
  Lock,
  RefreshCw,
  Trash2,
  Rss,
} from 'lucide-react';
import { useThemeStore } from '../../stores/themeStore';
import { useServerStore } from '../../stores/serverStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useRoutingStore } from '../../stores/routingStore';
import { MONO, SANS, pingColor, pingBg } from '../../lib/theme';
import { toast } from '../../stores/toastStore';
import type { ServerEntry } from '../../lib/types';

const PROFILES = [
  { id: 'gaming', name: 'Gaming', icon: Gamepad2 },
  { id: 'work', name: 'Work', icon: Briefcase },
  { id: 'smart', name: 'Smart Route', icon: Route },
  { id: 'full_vpn', name: 'Full Tunnel', icon: Lock },
];

export const Sidebar = () => {
  const T = useThemeStore((s) => s.theme);
  const {
    servers, subscriptions,
    fetchServers, fetchSubscriptions,
    addServer, addSubscription, removeSubscription, refreshSubscription,
    loading,
  } = useServerStore();
  const { status, activeServerId, connect, disconnect } = useConnectionStore();
  const { activeProfileId, setActiveProfile, profileLoading } = useRoutingStore();

  const [addOpen, setAddOpen] = useState(false);
  const [importUri, setImportUri] = useState('');

  useEffect(() => {
    fetchServers();
    fetchSubscriptions();
  }, [fetchServers, fetchSubscriptions]);

  useEffect(() => {
    if (status === 'connected' || status === 'disconnected') {
      fetchServers();
    }
  }, [status, fetchServers]);

  const isOn = status === 'connected';
  const isBusy = status === 'connecting' || status === 'disconnecting';

  const handleConnect = () => {
    if (isOn) {
      disconnect();
    } else if (!isBusy && activeServerId) {
      connect(activeServerId);
    } else if (!isBusy && servers.length > 0) {
      connect(servers[0].config.id);
    }
  };

  const handleServerClick = (server: ServerEntry) => {
    if (isOn && activeServerId === server.config.id) {
      disconnect();
    } else if (!isBusy) {
      connect(server.config.id);
    }
  };

  const handleImport = async () => {
    const input = importUri.trim();
    if (!input) {
      toast.warning('Paste a vless:// URI or subscription URL');
      return;
    }

    if (input.startsWith('http://') || input.startsWith('https://')) {
      await addSubscription(input);
    } else if (input.startsWith('vless://')) {
      const lines = input.split('\n').map((l) => l.trim()).filter((l) => l.startsWith('vless://'));
      for (const line of lines) {
        await addServer(line);
      }
    } else {
      toast.error('Invalid URI. Expected vless://... or subscription URL');
      return;
    }
    setImportUri('');
    setAddOpen(false);
  };

  const { routes } = useRoutingStore();
  const proxyCount = routes.filter((r) => r.mode === 'proxy').length;
  const directCount = routes.filter((r) => r.mode === 'direct').length;
  const blockCount = routes.filter((r) => r.mode === 'block').length;

  const timeAgo = (dateStr: string | null | undefined) => {
    if (!dateStr) return 'never';
    const diff = Date.now() - new Date(dateStr).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'just now';
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    return `${Math.floor(hrs / 24)}d ago`;
  };

  return (
    <div
      style={{
        width: 254,
        flexShrink: 0,
        display: 'flex',
        flexDirection: 'column',
        background: T.side,
        borderRight: `1px solid ${T.brd}`,
        overflow: 'hidden',
        fontFamily: SANS,
      }}
    >
      {/* Connect button */}
      <div style={{ padding: '10px 10px 8px' }}>
        <button
          onClick={handleConnect}
          style={{
            width: '100%',
            padding: '9px 0',
            border: 'none',
            borderRadius: 6,
            background: isOn ? T.ok : T.ac,
            color: '#fff',
            fontSize: 12,
            fontWeight: 600,
            fontFamily: SANS,
            cursor: 'pointer',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            gap: 7,
            transition: 'all .3s',
          }}
        >
          {isBusy ? (
            <>
              <Loader2 size={13} style={{ animation: 'spin .7s linear infinite' }} />
              Connecting…
            </>
          ) : isOn ? (
            <>
              <Wifi size={13} />
              Connected
            </>
          ) : (
            <>
              <WifiOff size={13} />
              Connect
            </>
          )}
        </button>
      </div>

      {/* Servers header */}
      <div
        style={{
          fontSize: 9,
          fontWeight: 600,
          textTransform: 'uppercase',
          letterSpacing: '.09em',
          color: T.t3,
          padding: '4px 12px 4px',
        }}
      >
        Servers
      </div>

      {/* Server list */}
      <div style={{ flex: 1, overflowY: 'auto' }}>
        {servers.map((server, i) => (
          <div key={server.config.id}>
            <button
              onClick={() => handleServerClick(server)}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '9px 12px',
                width: '100%',
                border: 'none',
                background:
                  activeServerId === server.config.id ? T.acS : 'transparent',
                cursor: 'pointer',
                textAlign: 'left',
                fontFamily: SANS,
                transition: 'all .12s',
                animation: `fu .2s ease ${i * 25}ms both`,
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: 9 }}>
                <div
                  style={{
                    width: 28,
                    height: 28,
                    borderRadius: 6,
                    background: T.bg2,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    fontSize: 9.5,
                    fontWeight: 700,
                    color: T.t1,
                    letterSpacing: '.02em',
                    flexShrink: 0,
                  }}
                >
                  {server.country?.toUpperCase() ?? '??'}
                </div>
                <div>
                  <div style={{ fontSize: 12, fontWeight: 500, color: T.t0 }}>
                    {server.display_name}
                  </div>
                  <div
                    style={{
                      fontSize: 9,
                      color: T.t3,
                      marginTop: 2,
                      display: 'flex',
                      alignItems: 'center',
                      gap: 4,
                    }}
                  >
                    <span
                      style={{
                        display: 'inline-block',
                        width: 6,
                        height: 2.5,
                        borderRadius: 2,
                        background: server.online ? T.ok : T.t3,
                      }}
                    />
                    {server.online ? 'Online' : 'Offline'}
                  </div>
                </div>
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                {server.ping_ms != null && (
                  <span
                    style={{
                      fontSize: 10,
                      fontWeight: 600,
                      fontFamily: MONO,
                      padding: '2px 5px',
                      borderRadius: 3,
                      color: pingColor(T, server.ping_ms),
                      background: pingBg(T, server.ping_ms),
                    }}
                  >
                    {server.ping_ms}
                  </span>
                )}
                {activeServerId === server.config.id && (
                  <Check size={11} style={{ color: T.ac }} />
                )}
              </div>
            </button>
            {i < servers.length - 1 && (
              <div
                style={{
                  height: 1,
                  background: T.brdSub,
                  marginLeft: 49,
                  marginRight: 12,
                }}
              />
            )}
          </div>
        ))}

        {servers.length === 0 && (
          <div
            style={{
              padding: '24px 12px',
              textAlign: 'center',
              fontSize: 11,
              color: T.t3,
            }}
          >
            No servers added yet
          </div>
        )}
      </div>

      {/* Add server */}
      <div style={{ padding: '6px 10px' }}>
        <button
          onClick={() => setAddOpen(!addOpen)}
          style={{
            width: '100%',
            padding: '6px 0',
            border: `1px dashed ${T.brd}`,
            borderRadius: 6,
            background: 'transparent',
            color: T.t2,
            fontSize: 10.5,
            fontWeight: 500,
            cursor: 'pointer',
            fontFamily: SANS,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            gap: 4,
          }}
        >
          {addOpen ? (
            <>
              <X size={11} />
              Cancel
            </>
          ) : (
            <>
              <Plus size={11} />
              Add Server
            </>
          )}
        </button>
        {addOpen && (
          <div
            style={{
              marginTop: 6,
              padding: 7,
              display: 'flex',
              flexDirection: 'column',
              gap: 5,
              background: T.bg1,
              borderRadius: 6,
              border: `1px solid ${T.brd}`,
              animation: 'si .12s ease',
            }}
          >
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
              <Copy size={11} style={{ color: T.t3, flexShrink: 0 }} />
              <input
                value={importUri}
                onChange={(e) => setImportUri(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleImport()}
                style={{
                  flex: 1,
                  border: 'none',
                  background: 'transparent',
                  color: T.t0,
                  fontSize: 10.5,
                  fontFamily: MONO,
                  outline: 'none',
                }}
                placeholder="vless://… or sub URL"
              />
            </div>
            <button
              onClick={handleImport}
              disabled={loading}
              style={{
                padding: '6px 0',
                border: 'none',
                borderRadius: 5,
                background: T.ac,
                color: '#fff',
                fontSize: 10.5,
                fontWeight: 600,
                cursor: 'pointer',
                fontFamily: SANS,
                opacity: loading ? 0.6 : 1,
              }}
            >
              {loading ? 'Importing…' : 'Import'}
            </button>
          </div>
        )}
      </div>

      {/* Subscriptions */}
      {subscriptions.length > 0 && (
        <div style={{ borderTop: `1px solid ${T.brd}` }}>
          <div
            style={{
              fontSize: 9,
              fontWeight: 600,
              textTransform: 'uppercase',
              letterSpacing: '.09em',
              color: T.t3,
              padding: '8px 12px 4px',
              display: 'flex',
              alignItems: 'center',
              gap: 5,
            }}
          >
            <Rss size={9} />
            Subscriptions
          </div>
          {subscriptions.map((sub) => (
            <SubRow
              key={sub.id}
              name={sub.name}
              serverCount={sub.servers?.length ?? 0}
              lastUpdated={timeAgo(sub.last_updated)}
              onRefresh={() => refreshSubscription(sub.id)}
              onRemove={() => {
                if (confirm(`Remove subscription "${sub.name}"?`)) {
                  removeSubscription(sub.id);
                }
              }}
            />
          ))}
        </div>
      )}

      {/* Profiles */}
      <div style={{ borderTop: `1px solid ${T.brd}` }}>
        <div
          style={{
            fontSize: 9,
            fontWeight: 600,
            textTransform: 'uppercase',
            letterSpacing: '.09em',
            color: T.t3,
            padding: '8px 12px 4px',
          }}
        >
          Profiles
        </div>
        {PROFILES.map((p, i) => {
          const Icon = p.icon;
          const active = activeProfileId === p.id;
          return (
            <div key={p.id}>
              <button
                onClick={() => !profileLoading && setActiveProfile(p.id)}
                disabled={profileLoading}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 7,
                  padding: '7px 12px',
                  width: '100%',
                  border: 'none',
                  background: active ? T.acS : 'transparent',
                  cursor: profileLoading ? 'wait' : 'pointer',
                  textAlign: 'left',
                  fontFamily: SANS,
                  fontSize: 11,
                  fontWeight: active ? 600 : 500,
                  color: active ? T.acT : T.t2,
                  transition: 'all .1s',
                  opacity: profileLoading && !active ? 0.5 : 1,
                }}
              >
                <Icon size={13} />
                <span style={{ flex: 1 }}>{p.name}</span>
                {active && <Radio size={9} style={{ color: T.ac }} />}
              </button>
              {i < PROFILES.length - 1 && (
                <div
                  style={{
                    height: 1,
                    background: T.brdSub,
                    marginLeft: 32,
                    marginRight: 12,
                  }}
                />
              )}
            </div>
          );
        })}
      </div>

      {/* Footer stats */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          padding: '8px 12px',
          borderTop: `1px solid ${T.brd}`,
        }}
      >
        {[
          { icon: Shield, count: proxyCount, label: 'proxy', color: T.mp },
          { icon: Globe, count: directCount, label: 'direct', color: T.ok },
          { icon: X, count: blockCount, label: 'block', color: T.er },
        ].map((item, i) => (
          <div key={item.label} style={{ display: 'contents' }}>
            {i > 0 && <div style={{ width: 1, height: 24, background: T.brd }} />}
            <div
              style={{
                flex: 1,
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                gap: 1,
              }}
            >
              <item.icon size={9} style={{ color: item.color }} />
              <span style={{ fontSize: 11.5, fontWeight: 600 }}>{item.count}</span>
              <span
                style={{
                  fontSize: 8,
                  color: T.t3,
                  textTransform: 'uppercase',
                  letterSpacing: '.05em',
                }}
              >
                {item.label}
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

/* ── Subscription row ── */

interface SubRowProps {
  name: string;
  serverCount: number;
  lastUpdated: string;
  onRefresh: () => void;
  onRemove: () => void;
}

const SubRow = ({ name, serverCount, lastUpdated, onRefresh, onRemove }: SubRowProps) => {
  const T = useThemeStore((s) => s.theme);
  const [hover, setHover] = useState(false);

  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        display: 'flex',
        alignItems: 'center',
        padding: '7px 12px',
        gap: 8,
        transition: 'background .1s',
        background: hover ? T.hover : 'transparent',
      }}
    >
      <Rss size={11} style={{ color: T.t3, flexShrink: 0 }} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            fontSize: 11,
            fontWeight: 500,
            color: T.t0,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {name}
        </div>
        <div style={{ fontSize: 9, color: T.t3, marginTop: 1 }}>
          {serverCount} servers · {lastUpdated}
        </div>
      </div>
      {hover && (
        <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
          <button
            onClick={onRefresh}
            title="Refresh"
            style={{
              border: 'none',
              background: T.bg2,
              borderRadius: 4,
              padding: 4,
              cursor: 'pointer',
              color: T.t2,
              display: 'flex',
            }}
          >
            <RefreshCw size={11} />
          </button>
          <button
            onClick={onRemove}
            title="Remove"
            style={{
              border: 'none',
              background: T.bg2,
              borderRadius: 4,
              padding: 4,
              cursor: 'pointer',
              color: T.er,
              display: 'flex',
            }}
          >
            <Trash2 size={11} />
          </button>
        </div>
      )}
    </div>
  );
};

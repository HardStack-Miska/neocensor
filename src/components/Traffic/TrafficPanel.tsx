import { useThemeStore } from '../../stores/themeStore';
import { useTrafficStore } from '../../stores/trafficStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { WaveChart } from '../common/WaveChart';
import { MONO, SANS, modeColor, modeBg } from '../../lib/theme';

export const TrafficPanel = () => {
  const T = useThemeStore((s) => s.theme);
  const status = useConnectionStore((s) => s.status);
  const { connections, totalCount, wavePoints } = useTrafficStore();

  const isConnected = status === 'connected';
  const hasData = connections.length > 0;

  return (
    <div
      style={{
        flex: 1,
        overflowY: 'auto',
        padding: '16px 20px',
        display: 'flex',
        flexDirection: 'column',
        gap: 14,
        fontFamily: SANS,
      }}
    >
      {/* Bandwidth chart */}
      <div
        style={{
          background: T.bg1,
          borderRadius: 8,
          border: `1px solid ${T.brd}`,
          padding: '14px 18px',
        }}
      >
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            marginBottom: 6,
          }}
        >
          <span style={{ fontSize: 13, fontWeight: 600 }}>Bandwidth</span>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <span style={{ fontSize: 10, fontFamily: MONO, color: T.t2 }}>
              {totalCount} connections
            </span>
            {isConnected && hasData && (
              <span
                style={{
                  fontSize: 10,
                  fontWeight: 600,
                  color: T.ok,
                  display: 'flex',
                  alignItems: 'center',
                  gap: 5,
                }}
              >
                <div
                  style={{
                    width: 5,
                    height: 5,
                    borderRadius: 3,
                    background: T.ok,
                    animation: 'br 2s ease infinite',
                  }}
                />
                Live
              </span>
            )}
          </div>
        </div>
        <WaveChart color={T.wave} points={wavePoints} />
      </div>

      {/* Connection log */}
      <div
        style={{
          flex: 1,
          background: T.bg1,
          borderRadius: 8,
          border: `1px solid ${T.brd}`,
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            padding: '10px 16px',
            borderBottom: `1px solid ${T.brd}`,
          }}
        >
          <span style={{ fontSize: 13, fontWeight: 600 }}>Connection Log</span>
          <span style={{ fontSize: 10, color: T.t3 }}>
            {hasData
              ? `${connections.length} entries`
              : 'Connect to see traffic'}
          </span>
        </div>

        {/* Header */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            padding: '6px 16px',
            fontSize: 9,
            fontWeight: 600,
            textTransform: 'uppercase',
            letterSpacing: '.06em',
            color: T.t3,
            borderBottom: `1px solid ${T.brd}`,
          }}
        >
          <span style={{ width: 62 }}>Time</span>
          <span style={{ flex: 1 }}>Destination</span>
          <span style={{ width: 50, textAlign: 'center' }}>Port</span>
          <span style={{ width: 60 }}>Route</span>
        </div>

        {/* Entries */}
        <div style={{ flex: 1, overflowY: 'auto' }}>
          {connections.length === 0 ? (
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                height: '100%',
                minHeight: 120,
                color: T.t3,
                fontSize: 11,
              }}
            >
              {isConnected
                ? 'Waiting for connections...'
                : 'Connect to a server to see live connections.'}
            </div>
          ) : (
            connections.map((entry, i) => (
              <div key={entry.id}>
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 10,
                    padding: '7px 16px',
                    animation: i < 10 ? `fu .12s ease ${i * 15}ms both` : undefined,
                  }}
                >
                  <span style={{ width: 62, fontFamily: MONO, fontSize: 10, color: T.t3 }}>
                    {entry.time}
                  </span>
                  <span
                    style={{
                      flex: 1,
                      fontSize: 11,
                      fontFamily: MONO,
                      color: T.t1,
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                  >
                    {entry.host}
                  </span>
                  <span
                    style={{
                      width: 50,
                      textAlign: 'center',
                      fontSize: 10,
                      fontFamily: MONO,
                      color: T.t3,
                    }}
                  >
                    {entry.port}
                  </span>
                  <span style={{ width: 60 }}>
                    <span
                      style={{
                        fontSize: 9.5,
                        fontWeight: 600,
                        padding: '2px 7px',
                        borderRadius: 3,
                        color: modeColor(T, entry.route),
                        background: modeBg(T, entry.route),
                      }}
                    >
                      {entry.route}
                    </span>
                  </span>
                </div>
                {i < connections.length - 1 && (
                  <div
                    style={{
                      height: 1,
                      background: T.brdSub,
                      marginLeft: 16,
                      marginRight: 16,
                    }}
                  />
                )}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
};

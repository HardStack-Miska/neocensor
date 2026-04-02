import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { Terminal, Trash2, FolderOpen } from 'lucide-react';
import { useThemeStore } from '../../stores/themeStore';
import { MONO, SANS } from '../../lib/theme';
import * as api from '../../lib/tauri';

const MAX_LINES = 500;

export const LogsPanel = () => {
  const T = useThemeStore((s) => s.theme);
  const [lines, setLines] = useState<string[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const [filter, setFilter] = useState('');
  const bottomRef = useRef<HTMLDivElement>(null);
  const started = useRef(false);

  useEffect(() => {
    if (started.current) return;
    started.current = true;

    api.startLogStream().catch(() => {});

    let cancelled = false;
    let unlistenFn: (() => void) | null = null;
    listen<string>('log-entry', (event) => {
      if (cancelled) return;
      setLines((prev) => {
        const next = [...prev, event.payload];
        return next.length > MAX_LINES ? next.slice(-MAX_LINES) : next;
      });
    }).then((fn) => {
      if (cancelled) { fn(); return; }
      unlistenFn = fn;
    });

    return () => { cancelled = true; unlistenFn?.(); };
  }, []);

  useEffect(() => {
    if (autoScroll && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [lines, autoScroll]);

  const filtered = filter
    ? lines.filter((l) => l.toLowerCase().includes(filter.toLowerCase()))
    : lines;

  const openLogDir = async () => {
    try {
      const path = await api.getLogPath();
      const opener = await import('@tauri-apps/plugin-opener');
      opener.openPath(path);
    } catch {
      // fallback
    }
  };

  const levelColor = (line: string) => {
    if (line.includes(' ERROR ') || line.includes(' error ')) return T.er;
    if (line.includes(' WARN ') || line.includes(' warn ')) return T.ma;
    if (line.includes(' INFO ') || line.includes(' info ')) return T.ok;
    return T.t2;
  };

  return (
    <div
      style={{
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        overflow: 'hidden',
        fontFamily: SANS,
      }}
    >
      {/* Toolbar */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 16px',
          borderBottom: `1px solid ${T.brd}`,
          flexShrink: 0,
        }}
      >
        <Terminal size={13} style={{ color: T.t3 }} />
        <span style={{ fontSize: 12, fontWeight: 600 }}>Application Logs</span>

        <div style={{ flex: 1 }} />

        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter…"
          style={{
            width: 160,
            padding: '4px 8px',
            borderRadius: 4,
            border: `1px solid ${T.brd}`,
            background: T.input,
            color: T.t0,
            fontSize: 10.5,
            fontFamily: MONO,
            outline: 'none',
          }}
        />

        <label
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 4,
            fontSize: 10,
            color: T.t2,
            cursor: 'pointer',
          }}
        >
          <input
            type="checkbox"
            checked={autoScroll}
            onChange={(e) => setAutoScroll(e.target.checked)}
            style={{ width: 12, height: 12 }}
          />
          Auto-scroll
        </label>

        <button
          onClick={() => setLines([])}
          style={{
            background: 'none',
            border: `1px solid ${T.brd}`,
            borderRadius: 4,
            padding: '3px 6px',
            cursor: 'pointer',
            color: T.t2,
            display: 'flex',
            alignItems: 'center',
            gap: 3,
            fontSize: 10,
          }}
        >
          <Trash2 size={10} /> Clear
        </button>

        <button
          onClick={openLogDir}
          style={{
            background: 'none',
            border: `1px solid ${T.brd}`,
            borderRadius: 4,
            padding: '3px 6px',
            cursor: 'pointer',
            color: T.t2,
            display: 'flex',
            alignItems: 'center',
            gap: 3,
            fontSize: 10,
          }}
        >
          <FolderOpen size={10} /> Open
        </button>
      </div>

      {/* Log output */}
      <div
        style={{
          flex: 1,
          overflowY: 'auto',
          padding: '4px 0',
          fontFamily: MONO,
          fontSize: 10.5,
          lineHeight: '18px',
          background: T.bg1,
        }}
      >
        {filtered.length === 0 ? (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              height: '100%',
              color: T.t3,
              fontSize: 11,
              fontFamily: SANS,
            }}
          >
            {lines.length === 0
              ? 'Waiting for log output…'
              : 'No matching entries'}
          </div>
        ) : (
          filtered.map((line, i) => (
            <div
              key={i}
              style={{
                padding: '1px 14px',
                color: levelColor(line),
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-all',
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLDivElement).style.background = T.hover;
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLDivElement).style.background = 'transparent';
              }}
            >
              {line}
            </div>
          ))
        )}
        <div ref={bottomRef} />
      </div>

      {/* Status bar */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '4px 14px',
          borderTop: `1px solid ${T.brd}`,
          fontSize: 9.5,
          color: T.t3,
        }}
      >
        <span>
          {filtered.length} / {lines.length} lines
        </span>
        <span>Max {MAX_LINES} lines in buffer</span>
      </div>
    </div>
  );
};

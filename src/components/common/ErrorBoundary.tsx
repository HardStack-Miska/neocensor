import React from 'react';
import { disconnectFromServer } from '../../lib/tauri';
import { useThemeStore } from '../../stores/themeStore';
import { SANS } from '../../lib/theme';

interface Props {
  children: React.ReactNode;
}

interface State {
  hasError: boolean;
  error: string | null;
}

export class ErrorBoundary extends React.Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error: error.message };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error('ErrorBoundary caught:', error, info);
  }

  render() {
    if (!this.state.hasError) {
      return this.props.children;
    }
    return <ErrorFallback error={this.state.error} />;
  }
}

const ErrorFallback = ({ error }: { error: string | null }) => {
  const T = useThemeStore((s) => s.theme);

  const handleReload = () => window.location.reload();
  const handleDisconnect = async () => {
    try { await disconnectFromServer(); } catch { /* best-effort */ }
  };

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100vh',
        background: T.bg0,
        color: T.t0,
        fontFamily: SANS,
        padding: 32,
        textAlign: 'center',
        gap: 16,
      }}
    >
      <div style={{ fontSize: 18, fontWeight: 600 }}>
        Something went wrong
      </div>
      <div style={{ fontSize: 13, color: T.t2, maxWidth: 400 }}>
        The application encountered an unexpected error.
        If VPN is active, you may need to disconnect manually.
      </div>
      {error && (
        <pre
          style={{
            fontSize: 11,
            color: T.er,
            background: T.bg1,
            padding: '8px 16px',
            borderRadius: 6,
            maxWidth: 500,
            overflow: 'auto',
            border: `1px solid ${T.brd}`,
          }}
        >
          {error}
        </pre>
      )}
      <div style={{ display: 'flex', gap: 12, marginTop: 8 }}>
        <button
          onClick={handleDisconnect}
          style={{
            padding: '8px 20px',
            border: `1px solid ${T.er}`,
            borderRadius: 6,
            background: 'transparent',
            color: T.er,
            cursor: 'pointer',
            fontSize: 13,
            fontWeight: 500,
            fontFamily: SANS,
          }}
        >
          Disconnect VPN
        </button>
        <button
          onClick={handleReload}
          style={{
            padding: '8px 20px',
            border: 'none',
            borderRadius: 6,
            background: T.ac,
            color: '#fff',
            cursor: 'pointer',
            fontSize: 13,
            fontWeight: 500,
            fontFamily: SANS,
          }}
        >
          Reload App
        </button>
      </div>
    </div>
  );
};

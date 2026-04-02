import type { ReactNode } from 'react';
import { useThemeStore } from '../../stores/themeStore';

interface SettingsGroupProps {
  title: string;
  children: ReactNode;
}

export const SettingsGroup = ({ title, children }: SettingsGroupProps) => {
  const T = useThemeStore((s) => s.theme);
  return (
    <div style={{ marginBottom: 22 }}>
      <div
        style={{
          fontSize: 9.5,
          fontWeight: 600,
          textTransform: 'uppercase',
          letterSpacing: '.08em',
          color: T.t3,
          marginBottom: 7,
          paddingLeft: 2,
        }}
      >
        {title}
      </div>
      <div
        style={{
          background: T.bg1,
          borderRadius: 8,
          border: `1px solid ${T.brd}`,
          overflow: 'hidden',
        }}
      >
        {children}
      </div>
    </div>
  );
};

interface SettingsRowProps {
  label: string;
  description?: string;
  last?: boolean;
  children: ReactNode;
}

export const SettingsRow = ({ label, description, last, children }: SettingsRowProps) => {
  const T = useThemeStore((s) => s.theme);
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '11px 14px',
        borderBottom: last ? 'none' : `1px solid ${T.brdSub}`,
        gap: 12,
      }}
    >
      <div>
        <div style={{ fontSize: 12.5, fontWeight: 500 }}>{label}</div>
        {description && (
          <div style={{ fontSize: 10, color: T.t2, marginTop: 1.5 }}>{description}</div>
        )}
      </div>
      {children}
    </div>
  );
};

interface SmallButtonProps {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
}

export const SmallButton = ({ children, onClick, disabled }: SmallButtonProps) => {
  const T = useThemeStore((s) => s.theme);
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 4,
        padding: '3px 8px',
        borderRadius: 4,
        border: `1px solid ${T.brd}`,
        background: T.bg2,
        color: T.t2,
        fontSize: 10,
        cursor: disabled ? 'default' : 'pointer',
        opacity: disabled ? 0.5 : 1,
        fontFamily: "'DM Sans',-apple-system,'Segoe UI',sans-serif",
      }}
    >
      {children}
    </button>
  );
};

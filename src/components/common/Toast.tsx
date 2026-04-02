import { CheckCircle, AlertTriangle, Info, X } from 'lucide-react';
import { useToastStore, type ToastType } from '../../stores/toastStore';
import { useThemeStore } from '../../stores/themeStore';
import { SANS } from '../../lib/theme';
import type { Theme } from '../../lib/theme';
import type { LucideIcon } from 'lucide-react';

const ICONS: Record<ToastType, LucideIcon> = {
  success: CheckCircle,
  error: AlertTriangle,
  warning: AlertTriangle,
  info: Info,
};

const ACCENT: Record<ToastType, (T: Theme) => string> = {
  success: (T) => T.ok,
  error: (T) => T.er,
  warning: (T) => T.ma,  // muted accent — no dedicated warning color in theme
  info: (T) => T.mp,
};

export const ToastContainer = () => {
  const T = useThemeStore((s) => s.theme);
  const toasts = useToastStore((s) => s.toasts);
  const remove = useToastStore((s) => s.removeToast);

  if (toasts.length === 0) return null;

  return (
    <div
      style={{
        position: 'fixed',
        bottom: 16,
        right: 16,
        zIndex: 10000,
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        maxWidth: 340,
        fontFamily: SANS,
      }}
    >
      {toasts.map((t) => {
        const Icon = ICONS[t.type];
        const accent = ACCENT[t.type](T);
        return (
          <div
            key={t.id}
            style={{
              display: 'flex',
              alignItems: 'flex-start',
              gap: 10,
              padding: '10px 12px',
              background: T.bg1,
              border: `1px solid ${T.brd}`,
              borderLeft: `3px solid ${accent}`,
              borderRadius: 8,
              boxShadow: T.shL,
              animation: 'fu .2s ease',
              color: T.t0,
              fontSize: 12,
              lineHeight: 1.45,
            }}
          >
            <Icon
              size={15}
              style={{ color: accent, flexShrink: 0, marginTop: 1 }}
            />
            <div style={{ flex: 1, wordBreak: 'break-word' }}>{t.message}</div>
            <button
              onClick={() => remove(t.id)}
              style={{
                border: 'none',
                background: 'none',
                color: T.t3,
                cursor: 'pointer',
                padding: 0,
                flexShrink: 0,
                marginTop: 1,
              }}
            >
              <X size={13} />
            </button>
          </div>
        );
      })}
    </div>
  );
};

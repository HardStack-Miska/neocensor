import type { LucideIcon } from 'lucide-react';

interface StatCardProps {
  label: string;
  value: string | number;
  color: string;
  bg: string;
  icon: LucideIcon;
}

export const StatCard = ({ label, value, color, bg, icon: Icon }: StatCardProps) => (
  <div
    style={{
      flex: 1,
      padding: '11px 13px',
      borderRadius: 7,
      background: bg,
      display: 'flex',
      flexDirection: 'column',
      gap: 5,
    }}
  >
    <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
      <Icon size={11} style={{ color }} />
      <span style={{ fontSize: 9.5, fontWeight: 500, opacity: 0.65 }}>{label}</span>
    </div>
    <span
      style={{
        fontSize: 16,
        fontWeight: 700,
        color,
        letterSpacing: '-0.02em',
      }}
    >
      {value}
    </span>
  </div>
);

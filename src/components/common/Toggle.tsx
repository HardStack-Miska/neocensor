import { useState } from 'react';
import { useThemeStore } from '../../stores/themeStore';

interface ToggleProps {
  value?: boolean;
  onChange?: (value: boolean) => void;
}

export const Toggle = ({ value, onChange }: ToggleProps) => {
  const T = useThemeStore((s) => s.theme);
  const [internal, setInternal] = useState(value ?? false);
  const isOn = onChange ? (value ?? false) : internal;

  const handle = () => {
    if (onChange) {
      onChange(!isOn);
    } else {
      setInternal(!internal);
    }
  };

  return (
    <button
      onClick={handle}
      style={{
        width: 38,
        height: 20,
        borderRadius: 10,
        border: 'none',
        cursor: 'pointer',
        background: isOn ? T.ac : T.bg3,
        position: 'relative',
        transition: 'background .25s',
        flexShrink: 0,
      }}
    >
      <div
        style={{
          width: 14,
          height: 14,
          borderRadius: 7,
          background: '#FFF',
          position: 'absolute',
          top: 3,
          left: isOn ? 21 : 3,
          transition: 'left .2s cubic-bezier(.4,0,.2,1)',
          boxShadow: '0 1px 2px rgba(0,0,0,.08)',
        }}
      />
    </button>
  );
};

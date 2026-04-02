interface WaveChartProps {
  color: string;
  /** Pre-computed data points for the wave chart */
  points: number[];
}

export const WaveChart = ({ color, points }: WaveChartProps) => {
  const mx = Math.max(...points, 1);
  const d = points
    .map(
      (p, i) =>
        `${i === 0 ? 'M' : 'L'} ${(i / (points.length - 1)) * 100} ${100 - (p / mx) * 90}`,
    )
    .join(' ');

  return (
    <svg
      viewBox="0 0 100 100"
      style={{ width: '100%', height: 100 }}
      preserveAspectRatio="none"
    >
      <defs>
        <linearGradient id="wf" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity=".15" />
          <stop offset="100%" stopColor={color} stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d={`${d} L 100 100 L 0 100 Z`} fill="url(#wf)" />
      <path
        d={d}
        fill="none"
        stroke={color}
        strokeWidth="1.2"
        vectorEffect="non-scaling-stroke"
        opacity=".6"
      />
    </svg>
  );
};

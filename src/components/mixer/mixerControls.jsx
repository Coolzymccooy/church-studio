/**
 * mixerControls.jsx — small shared, styled form controls reused by
 * ChannelStrip and BusStrip: a labeled horizontal slider, a toggle chip
 * button and the rotated vertical fader. Kept separate so each strip file
 * stays under the file-size guideline.
 */

export function LabeledSlider({
  label, value, min, max, step = 0.1, unit = '', onChange, ariaLabel, formatValue,
}) {
  const display = formatValue ? formatValue(value) : `${value >= 0 ? '+' : ''}${value.toFixed(1)}${unit}`;
  return (
    <div className="flex flex-col gap-0.5">
      <div className="flex items-center justify-between">
        <span className="text-[8px] font-bold uppercase tracking-wider text-slate-500">{label}</span>
        <span className="text-[8px] font-mono text-slate-400">{display}</span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(parseFloat(e.target.value))}
        aria-label={ariaLabel || label}
        aria-valuetext={display}
        className="w-full h-1 rounded appearance-none cursor-pointer accent-[var(--accent)]"
        style={{ background: 'rgba(148,163,184,0.25)' }}
      />
    </div>
  );
}

export function ToggleChip({ label, active, onClick, activeColor = 'var(--accent)', title }) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-pressed={active}
      className="flex-1 px-1 py-0.5 rounded text-[8px] font-bold border transition-all"
      style={active
        ? { background: `color-mix(in srgb, ${activeColor} 18%, transparent)`, borderColor: activeColor, color: activeColor }
        : { background: 'transparent', borderColor: '#1e293b', color: '#64748b' }}
    >
      {label}
    </button>
  );
}

export function VerticalFader({ position, onChange, ariaLabel, valueText, height = 148, width = 30 }) {
  return (
    <div className="relative flex items-center justify-center" style={{ width, height }}>
      <input
        type="range"
        min={0}
        max={1}
        step={0.001}
        value={position}
        onChange={(e) => onChange(parseFloat(e.target.value))}
        aria-label={ariaLabel}
        aria-valuetext={valueText}
        aria-orientation="vertical"
        className="cursor-pointer accent-[var(--accent)]"
        style={{
          position: 'absolute',
          width: height,
          height: 18,
          transform: 'rotate(-90deg)',
        }}
      />
    </div>
  );
}

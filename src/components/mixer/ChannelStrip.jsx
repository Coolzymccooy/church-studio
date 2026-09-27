import { useCallback, useEffect, useRef, useState } from 'react';
import { Sparkles } from 'lucide-react';

import { faderDbToPosition, formatDb, positionToFaderDb } from '../../lib/mixerEngine.js';
import { LabeledSlider, ToggleChip, VerticalFader } from './mixerControls.jsx';
import Meter from './Meter.jsx';

/**
 * GateGrIndicator — gate-open dot and gain-reduction readout, updated
 * imperatively from a requestAnimationFrame loop (never via setState) so the
 * 20 Hz mixer-meters stream never re-renders ChannelStrip.
 */
function GateGrIndicator({ getMeter }) {
  const dotRef = useRef(null);
  const textRef = useRef(null);

  useEffect(() => {
    let rafId = null;
    const tick = () => {
      const m = getMeter();
      if (dotRef.current) {
        dotRef.current.style.background = m.gate_open === false ? '#334155' : '#00E676';
      }
      if (textRef.current) {
        const gr = m.gr_db ?? 0;
        textRef.current.textContent = gr > 0.1 ? `GR-${gr.toFixed(1)}` : 'GR';
      }
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);
    return () => {
      if (rafId) cancelAnimationFrame(rafId);
    };
  }, [getMeter]);

  return (
    <div className="flex items-center gap-1">
      <span ref={dotRef} className="w-1.5 h-1.5 rounded-full" style={{ background: '#334155' }} title="Gate open" />
      <span ref={textRef} className="text-[7px] font-mono text-slate-500">GR</span>
    </div>
  );
}

/**
 * ChannelStrip — one input channel: name, voice-chain badge, trim + toggle
 * chips, 3-band EQ, three sends, pan, mute/solo, fader + pre/post meter.
 */
export default function ChannelStrip({ strip, metersRef, onSetParam, onSetBool, onRename }) {
  const [nameDraft, setNameDraft] = useState(strip.name);
  const [syncedName, setSyncedName] = useState(strip.name);
  // Reset the draft whenever the strip's committed name changes (e.g. a scene
  // load) without an Effect: adjust state directly during render, per React's
  // "you might not need an Effect" guidance.
  if (strip.name !== syncedName) {
    setSyncedName(strip.name);
    setNameDraft(strip.name);
  }

  const commitName = () => {
    const trimmed = nameDraft.trim();
    if (trimmed && trimmed !== strip.name) onRename(trimmed);
    else setNameDraft(strip.name);
  };

  // Memoized so GateGrIndicator's requestAnimationFrame effect (keyed on
  // this function's identity) isn't torn down and rebuilt on every
  // ChannelStrip re-render (e.g. every optimistic update while dragging).
  const meter = useCallback(
    () => metersRef.current.strips[strip.index] || {},
    [metersRef, strip.index],
  );
  const faderPosition = faderDbToPosition(strip.fader_db);

  return (
    <div
      className="w-[132px] shrink-0 flex flex-col gap-2 rounded-lg border border-slate-800 p-2"
      style={{ background: '#0D1428' }}
    >
      {/* Name */}
      <input
        value={nameDraft}
        maxLength={24}
        onChange={(e) => setNameDraft(e.target.value)}
        onBlur={commitName}
        onKeyDown={(e) => { if (e.key === 'Enter') e.currentTarget.blur(); }}
        aria-label={`Rename channel ${strip.index + 1}`}
        className="w-full px-1.5 py-1 rounded text-[10px] font-bold text-white border border-slate-700 focus:border-[var(--accent)] outline-none"
        style={{ background: '#050A1C' }}
      />

      {/* Voice AI badge */}
      <button
        type="button"
        onClick={() => onSetBool('voice_chain', !strip.voice_chain)}
        aria-pressed={strip.voice_chain}
        title="Route this channel through the AI voice-cleanup chain"
        className="w-full flex items-center justify-center gap-1 px-1 py-1 rounded text-[8px] font-bold border transition-all"
        style={strip.voice_chain
          ? { background: 'rgba(0,230,118,0.12)', borderColor: 'rgba(0,230,118,0.5)', color: '#00E676' }
          : { background: 'transparent', borderColor: '#1e293b', color: '#64748b' }}
      >
        <Sparkles size={10} /> VOICE AI
      </button>

      {/* Trim */}
      <LabeledSlider
        label="Trim" unit="dB" min={-20} max={40} step={0.5}
        value={strip.trim_db}
        onChange={(v) => onSetParam('trim_db', v)}
        ariaLabel={`${strip.name} trim`}
      />

      {/* Toggle chips */}
      <div className="flex gap-1">
        <ToggleChip label="HPF" active={strip.hpf_enabled} onClick={() => onSetBool('hpf_enabled', !strip.hpf_enabled)} title="High-pass filter" />
        <ToggleChip label="GATE" active={strip.gate_enabled} onClick={() => onSetBool('gate_enabled', !strip.gate_enabled)} title="Noise gate" />
        <ToggleChip label="COMP" active={strip.comp_enabled} onClick={() => onSetBool('comp_enabled', !strip.comp_enabled)} title="Compressor" />
        <ToggleChip label="Ø" active={strip.polarity} activeColor="#FFB800" onClick={() => onSetBool('polarity', !strip.polarity)} title="Invert polarity" />
      </div>

      {/* EQ */}
      <div className="rounded border border-slate-800 p-1.5 space-y-1" style={{ background: '#080E1F' }}>
        <div className="text-[7px] font-bold uppercase tracking-widest text-slate-600">EQ</div>
        <LabeledSlider label="Low" unit="" min={-15} max={15} step={0.5} value={strip.eq_low_db} onChange={(v) => onSetParam('eq_low_db', v)} ariaLabel={`${strip.name} low EQ`} />
        <LabeledSlider label="Mid" unit="" min={-15} max={15} step={0.5} value={strip.eq_mid_db} onChange={(v) => onSetParam('eq_mid_db', v)} ariaLabel={`${strip.name} mid EQ`} />
        <LabeledSlider label="High" unit="" min={-15} max={15} step={0.5} value={strip.eq_high_db} onChange={(v) => onSetParam('eq_high_db', v)} ariaLabel={`${strip.name} high EQ`} />
      </div>

      {/* Sends */}
      <div className="rounded border border-slate-800 p-1.5 space-y-1" style={{ background: '#080E1F' }}>
        <div className="text-[7px] font-bold uppercase tracking-widest text-slate-600">Sends</div>
        <LabeledSlider label="Main" unit="" min={-90} max={10} step={0.5} value={strip.send_main_db} onChange={(v) => onSetParam('send_main_db', v)} ariaLabel={`${strip.name} send to Main`} />
        <LabeledSlider label="Stream" unit="" min={-90} max={10} step={0.5} value={strip.send_stream_db} onChange={(v) => onSetParam('send_stream_db', v)} ariaLabel={`${strip.name} send to Stream`} />
        <LabeledSlider label="Mon" unit="" min={-90} max={10} step={0.5} value={strip.send_monitor_db} onChange={(v) => onSetParam('send_monitor_db', v)} ariaLabel={`${strip.name} send to Monitor`} />
        <button
          type="button"
          onClick={() => onSetBool('monitor_post_fader', !strip.monitor_post_fader)}
          aria-pressed={strip.monitor_post_fader}
          title="Monitor send taps pre-fader or post-fader"
          className="w-full text-[7px] font-bold py-0.5 rounded border border-slate-700 text-slate-400 hover:text-white"
        >
          MON: {strip.monitor_post_fader ? 'POST' : 'PRE'}
        </button>
      </div>

      {/* Pan */}
      <LabeledSlider
        label="Pan" min={-1} max={1} step={0.02}
        value={strip.pan}
        onChange={(v) => onSetParam('pan', v)}
        ariaLabel={`${strip.name} pan`}
        formatValue={(v) => (Math.abs(v) < 0.02 ? 'C' : v < 0 ? `L${Math.round(-v * 100)}` : `R${Math.round(v * 100)}`)}
      />

      {/* Mute / Solo */}
      <div className="flex gap-1">
        <button
          type="button"
          onClick={() => onSetBool('mute', !strip.mute)}
          aria-pressed={strip.mute}
          className="flex-1 py-1 rounded text-[9px] font-bold border transition-all"
          style={strip.mute
            ? { background: '#FF5252', borderColor: '#FF5252', color: '#fff' }
            : { background: 'transparent', borderColor: '#1e293b', color: '#94a3b8' }}
        >
          MUTE
        </button>
        <button
          type="button"
          onClick={() => onSetBool('solo', !strip.solo)}
          aria-pressed={strip.solo}
          title="Listen on Monitor only; never changes the live mix"
          className="flex-1 py-1 rounded text-[9px] font-bold border transition-all"
          style={strip.solo
            ? { background: '#FFB800', borderColor: '#FFB800', color: '#050A1C' }
            : { background: 'transparent', borderColor: '#1e293b', color: '#94a3b8' }}
        >
          PFL
        </button>
      </div>

      {/* Fader + meter */}
      <div className="flex items-end justify-center gap-2 pt-1">
        <div className="flex flex-col items-center gap-1">
          <VerticalFader
            position={faderPosition}
            onChange={(p) => onSetParam('fader_db', positionToFaderDb(p))}
            ariaLabel={`${strip.name} fader`}
          />
          <span className="text-[8px] font-mono text-slate-300">{formatDb(strip.fader_db)}</span>
        </div>
        <div className="flex flex-col items-center gap-1">
          <Meter
            bars={[
              { getDb: () => meter().pre_db ?? -96 },
              { getDb: () => meter().post_db ?? -96 },
            ]}
            width={22}
            height={148}
          />
          <GateGrIndicator getMeter={meter} />
        </div>
      </div>
    </div>
  );
}

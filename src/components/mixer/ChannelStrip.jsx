import { useCallback, useEffect, useRef, useState } from 'react';

import { faderDbToPosition, formatDb, positionToFaderDb } from '../../lib/mixerEngine.js';
import { STRIP_KNOBS, STRIP_ROWS, grText, stripColour, stripGateTone } from '../../lib/faceplateStrip.js';
import { Fader, Glass, Key, Knob, MeterLadder } from '../faceplate/index.js';

const FADER_HEIGHT = 132;
const LADDER_HEIGHT = FADER_HEIGHT - 18;
const FADER_MARKS = [10, 0, -10, -30].map((db) => ({ at: faderDbToPosition(db), zero: db === 0 }));

/**
 * Gate lamp and gain-reduction readout, written straight to the DOM from an animation frame so
 * the 20 Hz meter stream never re-renders the strip.
 */
function StripDyn({ getMeter, gateEnabled }) {
  const ledRef = useRef(null);
  const textRef = useRef(null);
  useEffect(() => {
    let raf = 0;
    const tick = () => {
      const m = getMeter();
      const tone = stripGateTone(gateEnabled, m.gate_open);
      if (ledRef.current) ledRef.current.className = tone === 'off' ? 'fp-led' : `fp-led fp-led-${tone}`;
      if (textRef.current) textRef.current.textContent = grText(m.gr_db);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [getMeter, gateEnabled]);
  return (
    <Glass className="fp-dyn" aria-label="gate and gain reduction">
      <span className="fp-lamp"><span ref={ledRef} className="fp-led" aria-hidden="true" /><span className="fp-glass-dim">gate</span></span>
      <span ref={textRef}>gr 0.0</span>
    </Glass>
  );
}

function KeyPair({ children }) {
  return <div className="fp-krow">{children}</div>;
}

/**
 * ChannelStrip — one input channel on the faceplate: scribble strip (click the name to rename),
 * polarity and high-pass, trim, filter frequency and pan, gate and compressor with their thresholds
 * and ratio, three-band EQ, three sends, monitor tap and voice chain, mute and PFL, the gate
 * and gain-reduction readout, and the fader between pre- and post-fader meters.
 */
export default function ChannelStrip({ strip, metersRef, onSetParam, onSetBool, onRename }) {
  const [nameDraft, setNameDraft] = useState(strip.name);
  const [syncedName, setSyncedName] = useState(strip.name);
  // Reset the draft whenever the committed name changes (e.g. a scene load), during render.
  if (strip.name !== syncedName) {
    setSyncedName(strip.name);
    setNameDraft(strip.name);
  }

  // Escape blurs the field to cancel; blur runs before the reset lands, so it must skip the commit.
  const cancelRename = useRef(false);
  const commitName = () => {
    if (cancelRename.current) {
      cancelRename.current = false;
      setNameDraft(strip.name);
      return;
    }
    const trimmed = nameDraft.trim();
    if (trimmed && trimmed !== strip.name) onRename(trimmed);
    else setNameDraft(strip.name);
  };

  // Stable so the meter loops are not rebuilt on every re-render while dragging.
  const meter = useCallback(() => metersRef.current.strips[strip.index] || {}, [metersRef, strip.index]);
  const preDb = useCallback(() => meter().pre_db ?? -96, [meter]);
  const postDb = useCallback(() => meter().post_db ?? -96, [meter]);

  const name = strip.name;
  const toggle = (key) => () => onSetBool(key, !strip[key]);
  const boolKey = (key, text, lit, label) => (
    <Key lit={strip[key] ? lit : null} pressed={strip[key]} onClick={toggle(key)} aria-label={`${name} ${label}`}>{text}</Key>
  );
  const knobView = (id) => {
    const k = STRIP_KNOBS[id];
    const value = strip[k.param];
    return (
      <Knob
        key={id}
        label={k.label}
        job={k.job}
        size="s"
        min={k.min}
        max={k.max}
        step={k.step}
        bipolar={k.bipolar}
        defaultValue={k.defaultValue}
        value={value}
        valueText={k.fmt(value)}
        off={k.off(strip)}
        onChange={(v) => onSetParam(k.param, v)}
      />
    );
  };
  const knobRow = (ids) => <div className={`fp-knobs fp-knobs-${ids.length}`}>{ids.map(knobView)}</div>;
  const [rowTrim, rowDyn, rowEq, rowSends] = STRIP_ROWS;

  return (
    <div className="fp-strip" role="group" aria-label={`Channel ${strip.index + 1} ${name}`}>
      <div className={`fp-scrib fp-c-${stripColour(strip.index)}${strip.mute ? ' fp-dim' : ''}`}>
        <span className="fp-no">{String(strip.index + 1).padStart(2, '0')}</span>
        <input
          value={nameDraft}
          maxLength={24}
          onChange={(e) => setNameDraft(e.target.value)}
          onBlur={commitName}
          onKeyDown={(e) => {
            if (e.key === 'Enter') e.currentTarget.blur();
            if (e.key === 'Escape') { cancelRename.current = true; e.currentTarget.blur(); }
          }}
          aria-label={`Rename channel ${strip.index + 1}`}
        />
      </div>

      <KeyPair>{boolKey('polarity', 'ø', 'sel', 'polarity')}{boolKey('hpf_enabled', 'hpf', 'green', 'high-pass')}</KeyPair>
      {knobRow(rowTrim)}
      <KeyPair>{boolKey('gate_enabled', 'gate', 'green', 'gate')}{boolKey('comp_enabled', 'comp', 'green', 'compressor')}</KeyPair>
      {knobRow(rowDyn)}
      {knobRow(rowEq)}
      {knobRow(rowSends)}
      <KeyPair>{boolKey('monitor_post_fader', 'post', 'sel', 'monitor send after fader')}{boolKey('voice_chain', 'voice', 'orange', 'voice chain')}</KeyPair>
      <KeyPair>{boolKey('mute', 'mute', 'amber', 'mute')}{boolKey('solo', 'pfl', 'sel', 'listen on monitor')}</KeyPair>

      <StripDyn getMeter={meter} gateEnabled={strip.gate_enabled} />

      <div className="fp-fz">
        <MeterLadder getDb={preDb} height={LADDER_HEIGHT} width={5} label={`${name} pre-fader level`} />
        <Fader
          position={faderDbToPosition(strip.fader_db)}
          onChange={(p) => onSetParam('fader_db', positionToFaderDb(p))}
          label={`${name} fader`}
          valueText={`${formatDb(strip.fader_db)} dB`}
          colour={stripColour(strip.index)}
          height={FADER_HEIGHT}
          marks={FADER_MARKS}
        />
        <MeterLadder getDb={postDb} height={LADDER_HEIGHT} width={5} label={`${name} post-fader level`} />
      </div>
      <span className="fp-fdb">{formatDb(strip.fader_db)}</span>
    </div>
  );
}

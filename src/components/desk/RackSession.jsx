import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';

const ms = (v, digits) => (v !== null && v !== undefined ? `${v.toFixed(digits)} ms` : '--');

function Telemetry({ stats }) {
  const cpuHigh = stats.cpuLoadPct !== null && stats.cpuLoadPct > 70;
  const rows = [
    ['latency', ms(stats.latencyMs, 1), ''],
    ['cpu', stats.cpuLoadPct !== null ? `${stats.cpuLoadPct.toFixed(0)}%` : '--', cpuHigh ? 'fp-caution' : ''],
    ['callback avg', ms(stats.callbackAvgMs, 2), ''],
    ['callback peak', ms(stats.callbackPeakMs, 2), ''],
    ['dropped samples', String(stats.droppedOutputSamples ?? 0), stats.droppedOutputSamples > 0 ? 'fp-fault' : 'fp-good'],
  ];
  return (
    <section className="fp-mod">
      <div className="fp-mod-hd">
        <span>Engine</span>
        <span className="fp-sub">{stats.state || 'idle'}</span>
      </div>
      <div className="fp-mod-bd">
        <div className="fp-glass" style={{ padding: 8, display: 'flex', flexDirection: 'column', gap: 4 }}>
          {rows.map(([label, value, cls]) => (
            <div key={label} className="fp-row"><span className="fp-glass-dim">{label}</span><span className={`fp-num ${cls}`}>{value}</span></div>
          ))}
        </div>
      </div>
    </section>
  );
}

function Routing({ s }) {
  return (
    <section className="fp-mod">
      <div className="fp-mod-hd">
        <span>Routing <InfoTip label="Routing">Input → TIWATON → broadcast device → OBS. Start the engine to open the outputs.</InfoTip></span>
        <Lamp tone={s.isLive ? 'g' : 'off'}>{s.isLive ? 'active' : 'idle'}</Lamp>
      </div>
      <div className="fp-mod-bd">
        <div className="fp-row"><span>Input</span><span className="fp-sub">{s.route?.input || 'waiting'}</span></div>
        <div className="fp-row"><span>Monitor</span><span className="fp-sub">{s.route?.monitor || s.monitorLabel}</span></div>
        <div className="fp-row"><span>Broadcast</span><span className="fp-sub">{s.route?.broadcast || s.broadcastLabel}</span></div>
      </div>
    </section>
  );
}

/** Session tab: routing, engine telemetry, A/B compare and session snapshots. */
export default function RackSession({ s }) {
  return (
    <>
      {s.showRouting && <Routing s={s} />}
      <Telemetry stats={s.stats} />

      <section className="fp-mod">
        <div className="fp-mod-hd">
          <span>A/B compare <InfoTip label="A/B compare">A is the raw input; B is the processed voice.</InfoTip></span>
        </div>
        <div className="fp-mod-bd">
          <div className="fp-keys fp-keys-fill" role="group" aria-label="A/B compare">
            <Key lit={s.abMode === 'A' ? 'amber' : null} pressed={s.abMode === 'A'} onClick={() => s.setBypassed(true)}>a · raw</Key>
            <Key lit={s.abMode === 'B' ? 'sel' : null} pressed={s.abMode === 'B'} onClick={() => s.setBypassed(false)}>b · processed</Key>
          </div>
        </div>
      </section>

      <section className="fp-mod">
        <div className="fp-mod-hd">
          <span>Snapshots <InfoTip label="Snapshots">Saves the whole voice setup so you can recall it.</InfoTip></span>
          <Key onClick={s.onSave}>save</Key>
        </div>
        <div className="fp-mod-bd">
          {s.snapshots.length === 0
            ? <span className="fp-sub">none saved</span>
            : s.snapshots.map((snap) => (
              <Key key={snap.id} className="fp-key-wide" onClick={() => s.onApply(snap)}
                style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span>{snap.label}</span><span>{snap.speakerPreset}</span>
              </Key>
            ))}
        </div>
      </section>
    </>
  );
}

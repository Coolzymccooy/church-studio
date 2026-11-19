import NdiInputPanel from '../NdiInputPanel.jsx';
import NdiOutputPanel from '../NdiOutputPanel.jsx';
import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';

const FLOW = [
  ['input', null],
  ['RNNoise', 'denoise'],
  ['hyper gate', 'denoise'],
  ['voice cleanse', 'voicePattern'],
  ['auto-mix', 'smartMixing'],
  ['warmth', 'sermonWarmth'],
  ['isolation', 'pastorIsolation'],
  ['voice polish', 'mastering'],
  ['limiter', 'streamingSafe'],
  ['output', null],
];

function SignalFlow({ isLive, hasSignal, features }) {
  return (
    <section className="fp-mod" aria-label="Signal flow">
      <div className="fp-mod-hd"><span>Signal flow</span></div>
      <div className="fp-mod-bd" style={{ gap: 4 }}>
        {FLOW.map(([label, key]) => {
          const on = key ? isLive && Boolean(features[key]) : hasSignal;
          return <Lamp key={label} tone={on ? 'g' : 'off'}>{label}</Lamp>;
        })}
      </div>
    </section>
  );
}

/** The right panel: monitor, broadcast, NDI out and in, the stream guard and the signal flow. */
export default function OutputRouter({ r }) {
  const monitorOn = r.monitoring && !r.reviewing;
  return (
    <aside className="fp-panel fp-panel-r" aria-label="Output router">
      <div className="fp-panel-hd"><span>Output router</span></div>
      <div className="fp-panel-bd">
        <section className="fp-mod">
          <div className="fp-mod-hd">
            <span>Monitor</span>
            <Key lit={monitorOn ? 'green' : null} pressed={monitorOn} disabled={r.reviewing} onClick={r.onToggleMonitor} aria-label="monitor">
              {monitorOn ? 'on' : 'off'}
            </Key>
          </div>
          <div className="fp-mod-bd">
            <div className="fp-row"><span className="fp-ellipsis" title={r.monitorLabel}>{r.monitorLabel}</span></div>
          </div>
        </section>

        <section className="fp-mod">
          <div className="fp-mod-hd">
            <span>Broadcast</span>
            <Lamp tone={r.isLive ? 'g' : 'off'}>{r.isLive ? 'live' : 'idle'}</Lamp>
          </div>
          <div className="fp-mod-bd">
            <div className="fp-row"><span className="fp-ellipsis" title={r.broadcastLabel}>{r.broadcastLabel}</span></div>
            <div className="fp-row">
              <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                For <InfoTip label="Broadcast app">The app that takes the broadcast output. Press to change.</InfoTip>
              </span>
              <Key onClick={r.onCycleTarget}>{r.targetApp}</Key>
            </div>
          </div>
        </section>

        <NdiOutputPanel isLive={r.isLive} />
        <NdiInputPanel isLive={r.isLive} />

        <SignalFlow isLive={r.isLive} hasSignal={r.isLive || r.isPlayingFile} features={r.features} />
      </div>
    </aside>
  );
}

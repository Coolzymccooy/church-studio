import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';
import ToggleRow from './ToggleRow.jsx';

const gainDb = (linear) => (linear > 0 ? 20 * Math.log10(linear) : -Infinity);
const dbText = (db) => (Number.isFinite(db) ? `${db.toFixed(1)} dB` : '-inf');

function gateLampState(core) {
  if (!core.features.denoise || !core.isLive) return { tone: 'off', word: 'off' };
  return core.gateClosed ? { tone: 'a', word: 'closed' } : { tone: 'g', word: 'open' };
}

/** Core tab: the input level and the Hyper Gate. */
export default function RackCore({ core }) {
  const gate = gateLampState(core);
  const signal = core.isLive || core.isPlayingFile;
  return (
    <>
      <section className="fp-mod">
        <div className="fp-mod-hd">
          <span>Input</span>
          <Lamp tone={signal ? 'g' : 'off'}>{signal ? 'signal' : 'idle'}</Lamp>
        </div>
        <div className="fp-mod-bd">
          <label className="fp-row" htmlFor="rack-input-level">
            <span>Level</span>
            <span className="fp-num">{dbText(gainDb(core.inputGain))}</span>
          </label>
          <input id="rack-input-level" type="range" className="fp-range" min="0" max="4" step="0.1"
            value={core.inputGain} onChange={core.onInputGainChange} />
          <div className="fp-row fp-sub"><span>Device</span><span>{core.deviceLabel}</span></div>
        </div>
      </section>

      <section className="fp-mod">
        <div className="fp-mod-hd">
          <span>Hyper gate <InfoTip label="Hyper gate">Closes the mic between words so room noise does not reach the stream.</InfoTip></span>
          <Lamp tone={gate.tone}>{gate.word}</Lamp>
        </div>
        <div className="fp-mod-bd">
          <div className="fp-keys fp-keys-fill" role="group" aria-label="Gate mode">
            <Key lit={core.gateMode === 'balanced' ? 'sel' : null} pressed={core.gateMode === 'balanced'} onClick={() => core.setGateMode('balanced')}>balanced</Key>
            <Key lit={core.gateMode === 'speech' ? 'sel' : null} pressed={core.gateMode === 'speech'} onClick={() => core.setGateMode('speech')}>speech only</Key>
          </div>
          <ToggleRow label="Reduction" on={Boolean(core.features.denoise)}
            onToggle={() => core.setFeatures((f) => ({ ...f, denoise: !f.denoise }))} />
          {core.features.denoise && (
            <>
              <label className="fp-row" htmlFor="rack-gate-threshold">
                <span>Threshold</span>
                <span className="fp-num">{core.threshold} dB</span>
              </label>
              <input id="rack-gate-threshold" type="range" className="fp-range" min="-100" max="-20" step="1"
                value={core.threshold} onChange={(e) => core.setThreshold(parseFloat(e.target.value))} />
              <div className="fp-keys fp-keys-fill">
                <Key onClick={core.onCalibrate} disabled={core.calibrating} lit={core.calibrating ? 'amber' : null}>
                  {core.calibrating ? 'calibrating' : 'auto-calibrate'}
                </Key>
                <Key onClick={core.onLearnMic} disabled={core.learning} lit={core.learning ? 'amber' : null}>
                  {core.learning ? 'learning' : core.micLearned ? 'relearn mic' : 'learn mic'}
                </Key>
              </div>
            </>
          )}
        </div>
      </section>
    </>
  );
}

import InfoTip from './faceplate/InfoTip.jsx';
import Key from './faceplate/Key.jsx';
import ToggleRow from './desk/ToggleRow.jsx';

const DSP_STAGES = [
  { key: 'lookaheadGate', label: 'Lookahead gate', tip: 'Opens the gate 20 ms early so the first syllable is not clipped.' },
  { key: 'dynamicDeEsser', label: 'De-esser', tip: 'Turns down harsh s and sh sounds only when they happen.', readout: 'deess' },
  { key: 'multibandComp', label: 'Multiband compressor', tip: 'Compresses sub, low, mid and air bands separately.' },
  { key: 'dereverb', label: 'Dereverb', tip: 'Reduces the room echo tail after each word.' },
];

const VOICE_STAGES = [
  { key: 'voicePattern', label: 'Voice cleanse', tip: 'Removes sound that does not match a voice.' },
  { key: 'musicDucking', label: 'Music ducking', tip: 'Lowers music while someone speaks.' },
  { key: 'pastorIsolation', label: 'Pastor isolation', tip: 'Favours the pastor over the choir.' },
  { key: 'sermonWarmth', label: 'Warmth', tip: 'Adds low-mid body to the voice.' },
  { key: 'smartMixing', label: 'Auto-mix', tip: 'Rides the voice level continuously.', readout: 'rider' },
  { key: 'mastering', label: 'Voice polish', tip: 'Broadcast loudness and presence.' },
];

const signedDb = (db) => `${db >= 0 ? '+' : ''}${db.toFixed(1)} dB`;

/** The rack's voice tab: every voice-chain stage as an on/off key, plus "all on" for a church service. */
export default function InputRackAiTab({ features, setFeatures, setGateMode, setSpeakerPreset, desserGrDb = 0, gainRiderDb = 0 }) {
  const toggle = (key) => setFeatures((prev) => ({ ...prev, [key]: !prev[key] }));

  const allOn = () => {
    setFeatures((prev) => ({
      ...prev,
      denoise: true,
      dereverb: true,
      voicePattern: true,
      musicDucking: true,
      pastorIsolation: true,
      sermonWarmth: true,
      smartMixing: true,
      mastering: true,
      streamingSafe: true,
      lookaheadGate: true,
      dynamicDeEsser: true,
      multibandComp: true,
    }));
    setGateMode('speech');
    setSpeakerPreset('pastor');
  };

  const readouts = {
    deess: desserGrDb < -0.5 ? signedDb(desserGrDb) : null,
    rider: features.smartMixing ? signedDb(gainRiderDb) : null,
  };

  const rows = (stages) => stages.map((s) => (
    <ToggleRow
      key={s.key}
      label={s.label}
      tip={s.tip}
      on={Boolean(features[s.key])}
      onToggle={() => toggle(s.key)}
      readout={s.readout ? readouts[s.readout] : null}
    />
  ));

  return (
    <>
      <section className="fp-mod">
        <div className="fp-mod-hd">
          <span>Church service</span>
          <InfoTip label="all on">Every stage on, speech-only gate and the pastor preset.</InfoTip>
        </div>
        <div className="fp-mod-bd">
          <Key className="fp-key-wide fp-key-tall" onClick={allOn}>all on</Key>
        </div>
      </section>
      <section className="fp-mod">
        <div className="fp-mod-hd"><span>Dynamics</span></div>
        <div className="fp-mod-bd">{rows(DSP_STAGES)}</div>
      </section>
      <section className="fp-mod">
        <div className="fp-mod-hd"><span>Voice</span></div>
        <div className="fp-mod-bd">{rows(VOICE_STAGES)}</div>
      </section>
    </>
  );
}

import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import ToggleRow from './ToggleRow.jsx';

const SPEAKER_PRESETS = [['balanced', 'balanced'], ['pastor', 'pastor'], ['guest', 'guest'], ['choir', 'choir lead']];
const TARGETS = [[-14, 'YouTube live, -14 LUFS'], [-16, 'Facebook live, -16 LUFS'], [-20, 'Zoom or Teams, -20 LUFS']];

/** Loudness tone by distance above target: fault 3 LU over, caution over, good otherwise. */
function lufsClass(value, target, faultMargin) {
  if (value === null) return '';
  if (value > target + faultMargin) return 'fp-fault';
  if (value > target) return 'fp-caution';
  return 'fp-good';
}

/** Profiles tab: room noise profile, speaker preset and the loudness target with its meter. */
export default function RackProfiles({ p }) {
  const fmt = (v, unit) => (v !== null ? `${v} ${unit}` : '--');
  return (
    <>
      <section className="fp-mod">
        <div className="fp-mod-hd"><span>Room</span></div>
        <div className="fp-mod-bd">
          <input type="text" className="fp-field" value={p.roomName} onChange={(e) => p.setRoomName(e.target.value)}
            placeholder="Room name" aria-label="Room name" />
          <div className="fp-row">
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
              Noise profile
              <InfoTip label="Noise profile">
                {p.isDesktop
                  ? 'Captures the room noise from recent input while the engine runs.'
                  : 'Pick a saved profile, or capture one from the current spectrum.'}
              </InfoTip>
            </span>
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
              {p.isDesktop && <span className="fp-sub">{p.nativeCaptured ? 'captured' : 'none'}</span>}
              <Key onClick={p.onCapture} disabled={!p.canCapture}>capture</Key>
            </span>
          </div>
          {!p.isDesktop && (
            <select className="fp-field" value={p.selectedProfileId} onChange={(e) => p.setSelectedProfileId(e.target.value)} aria-label="Noise profile">
              <option value="">No profile</option>
              {p.profiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.label}</option>)}
            </select>
          )}
        </div>
      </section>

      <section className="fp-mod">
        <div className="fp-mod-hd"><span>Speaker</span></div>
        <div className="fp-mod-bd">
          <div className="fp-keys fp-keys-fill" role="group" aria-label="Speaker preset">
            {SPEAKER_PRESETS.map(([value, label]) => (
              <Key key={value} lit={p.speakerPreset === value ? 'sel' : null} pressed={p.speakerPreset === value}
                onClick={() => p.setSpeakerPreset(value)}>{label}</Key>
            ))}
          </div>
        </div>
      </section>

      <section className="fp-mod">
        <div className="fp-mod-hd">
          <span>Loudness <InfoTip label="Loudness">ITU-R BS.1770-4, K-weighted.</InfoTip></span>
        </div>
        <div className="fp-mod-bd">
          <select className="fp-field" value={p.target} onChange={(e) => p.setTarget(Number(e.target.value))} aria-label="Loudness target">
            {TARGETS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </select>
          <div className="fp-glass" style={{ padding: 8, display: 'flex', flexDirection: 'column', gap: 4 }}>
            <div className="fp-row"><span className="fp-glass-dim">momentary</span><span className={`fp-num ${lufsClass(p.lufsM, p.target, 3)}`}>{fmt(p.lufsM, 'LU')}</span></div>
            <div className="fp-row"><span className="fp-glass-dim">short-term</span><span className={`fp-num ${p.lufsST !== null && p.lufsST > p.target + 2 ? 'fp-fault' : ''}`}>{fmt(p.lufsST, 'LUFS')}</span></div>
            <div className="fp-row"><span className="fp-glass-dim">integrated</span><span className="fp-num">{fmt(p.lufsI, 'LUFS')}</span></div>
            <div className="fp-row"><span className="fp-glass-dim">target</span><span className="fp-num">{p.target} LUFS</span></div>
          </div>
          <ToggleRow label="Stream safe" tip="Limits the stream so it never clips." tone="green"
            on={Boolean(p.streamSafe)} onToggle={p.onToggleStreamSafe} />
        </div>
      </section>
    </>
  );
}

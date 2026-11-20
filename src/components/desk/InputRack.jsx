import { AudioLines, Gauge, Mic, UserRound } from 'lucide-react';

import InputRackAiTab from '../InputRackAiTab.jsx';
import Key from '../faceplate/Key.jsx';
import TwoPressKey from '../faceplate/TwoPressKey.jsx';
import DeskRail from './DeskRail.jsx';
import RackCore from './RackCore.jsx';
import RackProfiles from './RackProfiles.jsx';
import RackSession from './RackSession.jsx';

const VOICE_KEYS = ['lookaheadGate', 'dynamicDeEsser', 'multibandComp', 'dereverb', 'voicePattern', 'musicDucking', 'pastorIsolation', 'sermonWarmth', 'smartMixing', 'mastering'];

/**
 * The input rack as a rail of faceplate icon keys: live or file source at the top, then input
 * (level and Hyper gate), voice, profiles and session. LEDs show signal, processing and health.
 */
export default function InputRack({ mode, onModeSwitch, core, voice, profiles, session }) {
  const signal = core.isLive || core.isPlayingFile;
  const voiceOn = VOICE_KEYS.some((k) => voice.features[k]);
  const stats = session.stats || {};
  const sessionWarn = session.isLive && ((stats.droppedOutputSamples ?? 0) > 0 || (stats.cpuLoadPct ?? 0) > 90);

  const items = [
    { id: 'input', icon: Mic, label: 'input', title: 'Input', led: signal ? 'g' : 'off', render: () => <RackCore core={core} /> },
    { id: 'voice', icon: AudioLines, label: 'voice', title: 'Voice', led: voiceOn ? 'o' : 'off', render: () => <InputRackAiTab {...voice} /> },
    { id: 'profiles', icon: UserRound, label: 'profiles', title: 'Profiles', led: profiles.streamSafe ? 'g' : 'off', render: () => <RackProfiles p={profiles} /> },
    { id: 'session', icon: Gauge, label: 'session', title: 'Session', led: sessionWarn ? 'a' : 'off', render: () => <RackSession s={session} /> },
  ];

  const top = (
    <>
      <div className="fp-mode-keys" role="group" aria-label="Source">
        <Key lit={mode === 'live' ? 'sel' : null} pressed={mode === 'live'} onClick={() => onModeSwitch('live')}>live</Key>
        {mode === 'live' && core.isLive
          ? (
            <TwoPressKey label="stop the engine and switch to file playback" onConfirm={() => onModeSwitch('file')}
              tip="Press twice within 3 seconds. File playback stops the live engine first.">
              file
            </TwoPressKey>
          )
          : <Key lit={mode === 'file' ? 'sel' : null} pressed={mode === 'file'} onClick={() => onModeSwitch('file')}>file</Key>}
      </div>
      <span className="fp-rail-sep" aria-hidden="true" />
    </>
  );

  return <DeskRail side="left" label="Input rack" items={items} top={top} />;
}

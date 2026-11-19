import { useState } from 'react';
import InputRackAiTab from '../InputRackAiTab.jsx';
import Key from '../faceplate/Key.jsx';
import RackCore from './RackCore.jsx';
import RackProfiles from './RackProfiles.jsx';
import RackSession from './RackSession.jsx';

const TABS = [
  { id: 'core', label: 'core' },
  { id: 'voice', label: 'voice' },
  { id: 'profiles', label: 'profiles' },
  { id: 'session', label: 'session' },
];

/** The left panel: live or file source, then the core, voice, profiles and session tabs. */
export default function InputRack({ mode, onModeSwitch, core, voice, profiles, session }) {
  const [tab, setTab] = useState('core');
  return (
    <aside className="fp-panel fp-panel-l" aria-label="Input rack">
      <div className="fp-panel-hd">
        <span>Input rack</span>
        <div className="fp-keys" role="group" aria-label="Source">
          <Key lit={mode === 'live' ? 'sel' : null} pressed={mode === 'live'} onClick={() => onModeSwitch('live')}>live</Key>
          <Key lit={mode === 'file' ? 'sel' : null} pressed={mode === 'file'} onClick={() => onModeSwitch('file')}>file</Key>
        </div>
      </div>
      <div className="fp-bar fp-keys-fill" role="tablist" aria-label="Rack sections" style={{ gap: 4 }}>
        {TABS.map((t) => (
          <Key key={t.id} role="tab" aria-selected={tab === t.id} lit={tab === t.id ? 'sel' : null} onClick={() => setTab(t.id)}>
            {t.label}
          </Key>
        ))}
      </div>
      <div className="fp-panel-bd">
        {tab === 'core' && <RackCore core={core} />}
        {tab === 'voice' && <InputRackAiTab {...voice} />}
        {tab === 'profiles' && <RackProfiles p={profiles} />}
        {tab === 'session' && <RackSession s={session} />}
      </div>
    </aside>
  );
}

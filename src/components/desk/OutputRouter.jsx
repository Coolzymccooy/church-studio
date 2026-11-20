import { Bell, Download, Headphones, RadioTower, Send, Workflow } from 'lucide-react';

import { alertLamp, deskAlerts } from '../../lib/deskRail.js';
import { NDI_TOOLS_URL } from '../../lib/ndiOutput';
import { openExternal } from '../../lib/platform';
import NdiInputPanel from '../NdiInputPanel.jsx';
import NdiOutputPanel from '../NdiOutputPanel.jsx';
import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';
import DeskRail from './DeskRail.jsx';
import useNdiRuntime from './useNdiRuntime.js';

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
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      {FLOW.map(([label, key]) => {
        const on = key ? isLive && Boolean(features[key]) : hasSignal;
        return <Lamp key={label} tone={on ? 'g' : 'off'}>{label}</Lamp>;
      })}
    </div>
  );
}

function AlertList({ alerts, onShowSettings, onMonitorOn }) {
  if (alerts.length === 0) return <Lamp tone="g">all clear</Lamp>;
  return alerts.map((a) => (
    <div key={a.id} className="fp-row" role={a.tone === 'r' ? 'alert' : undefined}>
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
        <Lamp tone={a.tone}>{a.text}</Lamp>
        <InfoTip label={a.text}>{a.tip}</InfoTip>
      </span>
      {a.action === 'setup' && <Key onClick={onShowSettings}>setup</Key>}
      {a.action === 'ndi-tools' && <Key onClick={() => { openExternal(NDI_TOOLS_URL).catch(() => {}); }}>get</Key>}
      {a.action === 'monitor' && <Key lit="green" onClick={onMonitorOn}>monitor on</Key>}
    </div>
  ));
}

/**
 * The output router as a rail of faceplate icon keys: monitor, broadcast, NDI out, NDI in,
 * signal flow and alerts. Each LED shows that module's state; a key opens the module beside it.
 */
export default function OutputRouter({ r }) {
  const monitorOn = r.monitoring && !r.reviewing;
  const ndi = useNdiRuntime();
  const ndiLed = ndi.checked && !ndi.available ? 'a' : 'off';
  const alerts = deskAlerts({
    engineError: r.engineError,
    ndi,
    broadcastSet: r.broadcastSet,
    isLive: r.isLive,
    droppedSamples: r.droppedSamples,
    cpuPct: r.cpuPct,
    lufsST: r.lufsST,
    target: r.target,
    computerMutedUnheard: Boolean(r.computerMutesSpeakers) && !monitorOn,
  });

  const items = [
    {
      id: 'monitor', icon: Headphones, label: 'monitor', title: 'Monitor', led: monitorOn ? 'g' : 'off',
      render: () => (
        <>
          <div className="fp-row">
            <span className="fp-ellipsis" title={r.monitorLabel}>{r.monitorLabel}</span>
            <Key lit={monitorOn ? 'green' : null} pressed={monitorOn} disabled={r.reviewing} onClick={r.onToggleMonitor} aria-label="monitor">
              {monitorOn ? 'on' : 'off'}
            </Key>
          </div>
          <Key onClick={r.onShowSettings}>change device</Key>
        </>
      ),
    },
    {
      id: 'cast', icon: RadioTower, label: 'cast', title: 'Broadcast',
      led: !r.broadcastSet ? 'a' : r.isLive ? 'g' : 'off',
      render: () => (
        <>
          <div className="fp-row">
            <span className="fp-ellipsis" title={r.broadcastLabel}>{r.broadcastLabel}</span>
            <Lamp tone={r.isLive && r.broadcastSet ? 'g' : 'off'}>{r.isLive && r.broadcastSet ? 'live' : 'idle'}</Lamp>
          </div>
          <div className="fp-row">
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
              For <InfoTip label="Broadcast app">The app that takes the broadcast output. Press to change.</InfoTip>
            </span>
            <Key onClick={r.onCycleTarget}>{r.targetApp}</Key>
          </div>
          <Key onClick={r.onShowSettings}>change device</Key>
        </>
      ),
    },
    { id: 'ndi-out', icon: Send, label: 'ndi out', title: 'Send to NDI®', led: ndiLed, render: () => <NdiOutputPanel isLive={r.isLive} /> },
    { id: 'ndi-in', icon: Download, label: 'ndi in', title: 'Receive NDI®', led: ndiLed, render: () => <NdiInputPanel isLive={r.isLive} /> },
    {
      id: 'flow', icon: Workflow, label: 'flow', title: 'Signal flow', led: r.isLive ? 'g' : 'off',
      render: () => <SignalFlow isLive={r.isLive} hasSignal={r.isLive || r.isPlayingFile} features={r.features} />,
    },
    {
      id: 'alerts', icon: Bell, label: 'alerts', title: 'Alerts', led: alertLamp(alerts), badge: alerts.length,
      aria: alerts.length ? `Alerts: ${alerts.length}` : 'Alerts: none',
      render: () => (
        <AlertList alerts={alerts} onShowSettings={r.onShowSettings}
          onMonitorOn={() => { if (!monitorOn) r.onToggleMonitor(); }} />
      ),
    },
  ];

  return <DeskRail side="right" label="Output router" items={items} />;
}

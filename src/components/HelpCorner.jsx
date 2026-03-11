import React, { useState } from 'react';
import { Check, X } from 'lucide-react';

function ExpandableCard({ icon, title, content }) {
  const [open, setOpen] = useState(false);
  return (
    <div
      className={`rounded-xl border overflow-hidden transition-all ${open ? 'border-amber-500/40' : 'border-slate-800 hover:border-slate-700'}`}
      style={{ background: '#0D1428' }}
    >
      <button
        onClick={() => setOpen((current) => !current)}
        className="w-full flex items-center gap-3 px-4 py-3 text-left"
      >
        <span className="text-xl leading-none shrink-0">{icon}</span>
        <span className="flex-1 text-[12px] font-semibold text-slate-200">{title}</span>
        <span className={`text-slate-500 text-lg leading-none transition-transform duration-200 ${open ? 'rotate-90' : ''}`}>›</span>
      </button>
      {open && (
        <div className="px-4 pb-3 pt-0 border-t border-slate-800/80">
          <p className="text-[11px] text-slate-400 leading-relaxed mt-2">{content}</p>
        </div>
      )}
    </div>
  );
}

function ChecklistPanel({ checklist }) {
  const [checked, setChecked] = useState({});
  const groups = [...new Set(checklist.map((entry) => entry.group))];
  const toggle = (step) => setChecked((prev) => ({ ...prev, [step]: !prev[step] }));
  const doneCount = checklist.filter((entry) => checked[entry.step]).length;
  const allDone = doneCount === checklist.length;

  return (
    <div className="p-4 space-y-5">
      <div>
        <div className="flex items-center justify-between mb-1.5">
          <p className="text-[10px] text-slate-500 uppercase tracking-wider font-bold">Sunday Pre-Service Checklist</p>
          <div className="flex items-center gap-2">
            <span className="text-[10px] font-mono text-slate-500">{doneCount}/{checklist.length}</span>
            <button onClick={() => setChecked({})} className="text-[9px] text-slate-700 hover:text-slate-400 transition-colors">Reset</button>
          </div>
        </div>
        <div className="h-1.5 bg-slate-800 rounded-full overflow-hidden">
          <div
            className="h-full rounded-full transition-all duration-500"
            style={{ width: `${(doneCount / checklist.length) * 100}%`, background: allDone ? '#00E676' : 'var(--accent)' }}
          />
        </div>
      </div>

      {groups.map((group) => (
        <div key={group}>
          <p className="text-[9px] font-bold text-slate-600 uppercase tracking-widest mb-2">{group}</p>
          <div className="space-y-2">
            {checklist.filter((entry) => entry.group === group).map(({ step }) => (
              <label
                key={step}
                onClick={() => toggle(step)}
                className="flex items-start gap-3 cursor-pointer group"
              >
                <div
                  className={`mt-0.5 w-4 h-4 rounded border-2 flex items-center justify-center shrink-0 transition-all ${
                    checked[step]
                      ? 'bg-[#00E676] border-[#00E676]'
                      : 'border-slate-700 bg-slate-900 group-hover:border-slate-500'
                  }`}
                >
                  {checked[step] && <Check size={10} className="text-black" strokeWidth={3} />}
                </div>
                <span className={`text-[11px] leading-snug transition-colors ${checked[step] ? 'text-slate-600 line-through' : 'text-slate-300 group-hover:text-slate-200'}`}>
                  {step}
                </span>
              </label>
            ))}
          </div>
        </div>
      ))}

      {allDone && (
        <div className="rounded-xl border border-emerald-500/30 px-4 py-3 text-center" style={{ background: 'rgba(0,230,118,0.07)' }}>
          <p className="text-[13px] font-bold text-emerald-400">All clear! You're ready to go live.</p>
          <p className="text-[10px] text-emerald-700 mt-0.5">Press the amber button to start the engine.</p>
        </div>
      )}
    </div>
  );
}

export default function HelpCorner() {
  const [isOpen, setIsOpen] = useState(false);
  const [activeTab, setActiveTab] = useState('fixes');

  const quickFixes = [
    {
      icon: '🔇',
      title: 'No sound on stream',
      content: 'Make sure LIVE is ON. In OBS choose VB-CABLE Input or BlackHole, not your physical microphone.',
    },
    {
      icon: '😤',
      title: 'Too much background noise / HVAC hum',
      content: 'Enable Hyper-Gate and RNNoise, then run Auto-Calibrate Gate while the room is at its normal noise level.',
    },
    {
      icon: '🔁',
      title: 'Echo / hearing voice twice',
      content: 'Remove every OBS audio source except the TIWATON virtual cable. Do not send both raw mic and processed audio.',
    },
    {
      icon: '✂️',
      title: 'Voice getting cut off mid-sentence',
      content: 'Lower the gate threshold or run Auto-Calibrate again. If needed, disable Hyper-Gate temporarily.',
    },
    {
      icon: '📻',
      title: 'Audio sounds thin or tinny',
      content: 'Enable Sermon Warmth and Voice Polish. In the editor, use the Sermon Warm preset before exporting WAV.',
    },
    {
      icon: '🔴',
      title: 'Clipping / levels too hot',
      content: 'Reduce gain on the mixer or interface first, then lower input gain in TIWATON.',
    },
    {
      icon: '💻',
      title: 'App crashed or frozen',
      content: 'Click Reset, wait a few seconds, then start Live again. Avoid changing devices while the engine is live.',
    },
    {
      icon: '🎙',
      title: 'Making a sermon recording sound podcast-quality',
      content: 'Import the recording in the Audio Editor, choose a mastering preset, run Sermon Master, then export WAV.',
    },
  ];

  const checklist = [
    { step: 'Mic plugged into mixer or USB interface', group: 'Before You Start' },
    { step: 'Gain set correctly — no red clipping on input', group: 'Before You Start' },
    { step: 'System mic enhancements turned OFF', group: 'Before You Start' },
    { step: 'TIWATON input source set to your interface', group: 'TIWATON Setup' },
    { step: 'Broadcast Output set to VB-CABLE or BlackHole', group: 'TIWATON Setup' },
    { step: 'Hyper-Gate, Voice Isolation, Warmth, Auto-Mix all enabled', group: 'TIWATON Setup' },
    { step: 'Auto-Calibrate Gate run with normal room noise', group: 'TIWATON Setup' },
    { step: 'OBS audio source set to the same virtual cable', group: 'OBS / Streaming' },
    { step: 'All raw microphone inputs removed from OBS', group: 'OBS / Streaming' },
    { step: '10-second test recording sounds clear', group: 'Sound Check' },
    { step: 'Press LIVE — status shows green Live Processing', group: 'Go Live' },
  ];

  const setupSteps = [
    {
      num: '1',
      title: 'Physical Connection',
      color: 'var(--accent)',
      items: [
        'Pastor mic to mixer or audio interface, not directly into the laptop',
        'One clean USB cable from interface into computer',
        'Disable OS noise suppression and AGC',
        'Monitor on headphones, not speakers',
      ],
    },
    {
      num: '2',
      title: 'Inside TIWATON',
      color: '#00E676',
      items: [
        'Select the interface as the input source',
        'Set Broadcast Output to VB-CABLE Input or BlackHole',
        'Enable Hyper-Gate, Voice Isolation, Warmth, and Auto-Mix',
        'Run Auto-Calibrate Gate with normal room noise',
      ],
    },
    {
      num: '3',
      title: 'In OBS / StreamYard',
      color: '#60a5fa',
      items: [
        'Choose the TIWATON broadcast device as Audio Input Capture',
        'Remove physical mic and webcam sources',
        'Use the same broadcast device in both TIWATON and OBS',
        'Confirm OBS meters move when the pastor speaks',
      ],
    },
  ];

  return (
    <>
      <button
        type="button"
        data-help-button
        onClick={() => setIsOpen(true)}
        className="fixed bottom-4 left-16 z-40 flex items-center gap-2 rounded-full px-3 py-2 text-[11px] font-semibold text-slate-100 shadow-lg border transition-all hover:scale-105"
        style={{ background: '#0D1428', borderColor: 'rgba(var(--accent-rgb),0.35)' }}
        title="Audio help and Sunday checklist"
      >
        <span className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-amber-500 text-[10px] font-bold text-black">?</span>
        <span className="hidden sm:inline text-slate-400">Help</span>
      </button>

      {isOpen && (
        <div className="fixed inset-0 z-50 flex justify-end">
          <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={() => setIsOpen(false)} />

          <div className="relative flex flex-col w-full max-w-md h-full border-l border-slate-700 shadow-2xl overflow-hidden" style={{ background: '#080E1F' }}>
            <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-800 shrink-0" style={{ background: '#030710' }}>
              <div className="w-8 h-8 rounded-lg flex items-center justify-center font-bold text-sm text-black" style={{ background: 'var(--accent)' }}>?</div>
              <div className="flex-1">
                <p className="text-[13px] font-bold text-slate-100">Audio Help Center</p>
                <p className="text-[10px] text-slate-500">Quick fixes, setup guide, Sunday checklist</p>
              </div>
              <button onClick={() => setIsOpen(false)} className="p-1.5 rounded-lg text-slate-500 hover:text-white hover:bg-slate-800 transition-colors">
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="flex border-b border-slate-800 shrink-0" style={{ background: '#030710' }}>
              {[
                { id: 'fixes', label: 'Quick Fixes' },
                { id: 'setup', label: 'Setup Guide' },
                { id: 'checklist', label: 'Checklist' },
              ].map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id)}
                  className={`flex-1 py-2.5 text-[11px] font-semibold transition-colors ${
                    activeTab === tab.id
                      ? 'text-amber-400 border-b-2 border-amber-500'
                      : 'text-slate-500 hover:text-slate-300'
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </div>

            <div className="flex-1 overflow-y-auto">
              {activeTab === 'fixes' && (
                <div className="p-4 space-y-2">
                  <p className="text-[10px] text-slate-600 uppercase tracking-wider mb-3">Tap your problem for an instant fix</p>
                  {quickFixes.map((card) => (
                    <ExpandableCard key={card.title} icon={card.icon} title={card.title} content={card.content} />
                  ))}
                </div>
              )}

              {activeTab === 'setup' && (
                <div className="p-4 space-y-4">
                  <p className="text-[10px] text-slate-600 uppercase tracking-wider mb-1">Follow these 3 steps every Sunday</p>

                  {setupSteps.map(({ num, title, color, items }) => (
                    <div key={num} className="rounded-xl border border-slate-800 overflow-hidden" style={{ background: '#0D1428' }}>
                      <div className="flex items-center gap-2.5 px-3 py-2.5 border-b border-slate-800" style={{ background: 'rgba(255,255,255,0.03)' }}>
                        <div className="w-6 h-6 rounded-full flex items-center justify-center text-[11px] font-bold text-black shrink-0" style={{ background: color }}>{num}</div>
                        <span className="font-bold text-[12px] text-slate-100">{title}</span>
                      </div>
                      <ul className="px-4 py-3 space-y-2">
                        {items.map((item) => (
                          <li key={item} className="flex items-start gap-2 text-[11px] text-slate-400 leading-snug">
                            <span className="text-slate-700 mt-0.5 shrink-0">›</span>
                            <span>{item}</span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  ))}
                </div>
              )}

              {activeTab === 'checklist' && <ChecklistPanel checklist={checklist} />}
            </div>

            <div className="border-t border-slate-800 px-4 py-2 shrink-0 text-center" style={{ background: '#030710' }}>
              <p className="text-[9px] text-slate-700">TIWATON Audio Help · Built for churches and live streamers</p>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

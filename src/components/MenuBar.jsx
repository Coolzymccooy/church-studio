import React, { useState, useRef, useEffect } from 'react';
import { Waves } from 'lucide-react';

/**
 * MenuBar — Native-style menu bar for TIWATON AI STUDIO.
 * Renders a desktop app menu bar with dropdown menus.
 */

function DropdownMenu({ label, items, isOpen, onToggle, onClose }) {
  const menuRef = useRef(null);

  useEffect(() => {
    if (!isOpen) return;
    const handleClick = (e) => {
      if (menuRef.current && !menuRef.current.contains(e.target)) onClose();
    };
    window.addEventListener('mousedown', handleClick);
    return () => window.removeEventListener('mousedown', handleClick);
  }, [isOpen, onClose]);

  return (
    <div ref={menuRef} className="relative">
      <button
        onClick={onToggle}
        className={`px-2.5 py-1 rounded text-[11px] font-medium transition-colors ${
          isOpen
            ? 'bg-[#4F7BFF]/20 text-white'
            : 'text-slate-400 hover:text-white hover:bg-white/5'
        }`}
      >
        {label}
      </button>

      {isOpen && (
        <div
          className="absolute left-0 top-full mt-0.5 min-w-[200px] rounded-lg border border-slate-700 shadow-2xl z-50 overflow-hidden py-1"
          style={{ background: '#0D1428' }}
        >
          {items.map((item, i) => {
            if (item.separator) {
              return <div key={i} className="my-1 border-t border-slate-800" />;
            }
            return (
              <button
                key={i}
                disabled={item.disabled}
                onClick={() => {
                  if (item.action && !item.disabled) {
                    item.action();
                    onClose();
                  }
                }}
                className={`w-full flex items-center justify-between px-4 py-1.5 text-[11px] transition-colors text-left ${
                  item.disabled
                    ? 'text-slate-600 cursor-not-allowed'
                    : 'text-slate-300 hover:bg-[#4F7BFF]/15 hover:text-white cursor-pointer'
                }`}
              >
                <span className="flex items-center gap-2">
                  {item.icon && <span className="opacity-60">{item.icon}</span>}
                  {item.label}
                </span>
                {item.shortcut && (
                  <span className="text-[10px] text-slate-600 font-mono ml-6">{item.shortcut}</span>
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

export default function MenuBar({
  // Callbacks from parent
  onGoHome,
  onShowSettings,
  onToggleLive,
  onHardReset,
  onStartAutoCalibrate,
  onStartMicLearn,
  onCaptureNoise,
  onHandleModeSwitch,
  onSetMainTab,
  onSnapshotSave,
  onExportMp4,
  onShareRecording,
  onDownloadWaveform,
  // State
  isLive,
  mode,
  mainTab,
  recordingState,
  isBypassed,
  setIsBypassed,
  features,
  setFeatures,
}) {
  const [openMenu, setOpenMenu] = useState(null);

  const toggle = (name) => setOpenMenu(prev => prev === name ? null : name);
  const close = () => setOpenMenu(null);

  const menus = [
    {
      id: 'file',
      label: 'File',
      items: [
        { label: 'New Session', shortcut: 'Ctrl+N', action: onHardReset },
        { label: 'Audio Device Settings', shortcut: 'Ctrl+,', action: onShowSettings },
        { separator: true },
        { label: 'Export WAV Recording', action: () => onShareRecording?.('wav'), disabled: recordingState !== 'review' },
        { label: 'Export WebM Recording', action: () => onShareRecording?.('webm'), disabled: recordingState !== 'review' },
        { label: 'Export MP4 (Canvas + Audio)', action: onExportMp4, disabled: !isLive },
        { label: 'Spectrum Snapshot (PNG)', action: onDownloadWaveform },
        { separator: true },
        { label: 'Save Session Snapshot', shortcut: 'Ctrl+S', action: onSnapshotSave },
        { separator: true },
        { label: 'Go Home', action: onGoHome },
      ],
    },
    {
      id: 'engine',
      label: 'Engine',
      items: [
        { label: isLive ? '⏹ Stop Engine' : '▶ Start Engine', shortcut: 'Space', action: onToggleLive },
        { separator: true },
        { label: 'Restart / Hard Reset', action: onHardReset },
        { label: 'Audio Device Settings', action: onShowSettings },
        { separator: true },
        { label: 'Switch to Live Input', action: () => onHandleModeSwitch?.('live'), disabled: mode === 'live' },
        { label: 'Switch to File Mode', action: () => onHandleModeSwitch?.('file'), disabled: mode === 'file' },
      ],
    },
    {
      id: 'view',
      label: 'View',
      items: [
        { label: '● Live Visualizer', action: () => onSetMainTab?.('live'), disabled: mainTab === 'live' },
        { label: '◈ Audio Editor', action: () => onSetMainTab?.('editor'), disabled: mainTab === 'editor' },
        { separator: true },
        { label: isBypassed ? 'Disable Bypass (B)' : 'Enable Bypass (B)', action: () => setIsBypassed?.(!isBypassed) },
      ],
    },
    {
      id: 'ai',
      label: 'AI Stack',
      items: [
        {
          label: 'Church Beta — All ON',
          action: () => setFeatures?.({ denoise: true, dereverb: true, voicePattern: true, musicDucking: true, pastorIsolation: true, sermonWarmth: true, smartMixing: true, mastering: true, streamingSafe: true }),
        },
        { separator: true },
        { label: `${features?.denoise ? '✓' : '  '} Hyper-Gate`, action: () => setFeatures?.(f => ({ ...f, denoise: !f.denoise })) },
        { label: `${features?.pastorIsolation ? '✓' : '  '} Pastor Voice Isolation`, action: () => setFeatures?.(f => ({ ...f, pastorIsolation: !f.pastorIsolation })) },
        { label: `${features?.sermonWarmth ? '✓' : '  '} Sermon Warmth`, action: () => setFeatures?.(f => ({ ...f, sermonWarmth: !f.sermonWarmth })) },
        { label: `${features?.mastering ? '✓' : '  '} Voice Polish`, action: () => setFeatures?.(f => ({ ...f, mastering: !f.mastering })) },
        { label: `${features?.dereverb ? '✓' : '  '} Echo/Reverb Cleaner`, action: () => setFeatures?.(f => ({ ...f, dereverb: !f.dereverb })) },
        { label: `${features?.musicDucking ? '✓' : '  '} Music Ducking`, action: () => setFeatures?.(f => ({ ...f, musicDucking: !f.musicDucking })) },
        { label: `${features?.smartMixing ? '✓' : '  '} Smart Auto-Mixing`, action: () => setFeatures?.(f => ({ ...f, smartMixing: !f.smartMixing })) },
        { separator: true },
        { label: isBypassed ? '✓ AI Bypassed (Raw)' : '  Bypass AI Processing', action: () => setIsBypassed?.(!isBypassed) },
      ],
    },
    {
      id: 'tools',
      label: 'Tools',
      items: [
        { label: 'Auto-Calibrate Gate', action: onStartAutoCalibrate, disabled: !isLive },
        { label: 'Learn Mic Fingerprint (5s)', action: onStartMicLearn, disabled: !isLive },
        { label: 'Capture Room Noise Profile', action: onCaptureNoise, disabled: !isLive },
        { separator: true },
        { label: 'Audio Device Settings', shortcut: 'Ctrl+,', action: onShowSettings },
      ],
    },
    {
      id: 'help',
      label: 'Help',
      items: [
        { label: 'TIWATON Setup Guide', action: () => document.querySelector('[data-help-button]')?.click() },
        { label: 'Routing: OBS + Virtual Cable', action: () => document.querySelector('[data-help-button]')?.click() },
        { separator: true },
        { label: 'Version: TIWATON AI Studio v1.1', disabled: true },
        { label: '26-node DSP Pipeline', disabled: true },
      ],
    },
  ];

  return (
    <div
      className="h-9 shrink-0 flex items-center px-2 gap-1 z-30 border-b border-slate-800/60"
      style={{ background: '#030710', fontFamily: 'Inter, system-ui, sans-serif' }}
    >
      {/* Logo */}
      <div
        className="flex items-center gap-2 px-2 mr-3 cursor-pointer group"
        onClick={onGoHome}
        title="Back to Home"
      >
        <div className="w-5 h-5 bg-indigo-600 rounded flex items-center justify-center shadow-[0_0_8px_rgba(79,70,229,0.5)] group-hover:scale-105 transition-transform">
          <Waves size={11} className="text-white" />
        </div>
        <span className="text-slate-200 font-bold text-[11px] tracking-[0.2em]">TIWATON</span>
        <span className="text-slate-600 text-[10px] hidden sm:inline">AI STUDIO</span>
      </div>

      <div className="h-4 w-px bg-slate-800 mr-1" />

      {/* Menu items */}
      {menus.map(menu => (
        <DropdownMenu
          key={menu.id}
          label={menu.label}
          items={menu.items}
          isOpen={openMenu === menu.id}
          onToggle={() => toggle(menu.id)}
          onClose={close}
        />
      ))}

      {/* Right side — mode switcher */}
      <div className="ml-auto flex items-center gap-2">
        <div
          className="flex items-center rounded p-0.5 border border-slate-800 gap-0.5"
          style={{ background: '#050A1C' }}
        >
          <button
            onClick={() => onHandleModeSwitch?.('live')}
            className={`px-2.5 py-0.5 rounded text-[10px] font-semibold transition-all ${
              mode === 'live' ? 'bg-[#4F7BFF] text-white' : 'text-slate-500 hover:text-white'
            }`}
          >
            Live
          </button>
          <button
            onClick={() => onHandleModeSwitch?.('file')}
            className={`px-2.5 py-0.5 rounded text-[10px] font-semibold transition-all ${
              mode === 'file' ? 'bg-[#4F7BFF] text-white' : 'text-slate-500 hover:text-white'
            }`}
          >
            File
          </button>
        </div>
      </div>
    </div>
  );
}

import { useState, useRef, useEffect } from 'react';
import { dragRegionProps, isTauri } from '../lib/platform';

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
      <button type="button" onClick={onToggle} className="fp-menu-btn" aria-expanded={isOpen} aria-haspopup="menu">
        {label}
      </button>

      {isOpen && (
        <div className="fp-menu" role="menu" aria-label={label}>
          {items.map((item, i) => {
            if (item.separator) {
              return <div key={i} className="fp-menu-sep" role="separator" />;
            }
            return (
              <button
                key={i}
                type="button"
                role="menuitem"
                disabled={item.disabled}
                onClick={() => {
                  if (item.action && !item.disabled) {
                    item.action();
                    onClose();
                  }
                }}
                className="fp-menu-item"
              >
                <span>{item.label}</span>
                {item.shortcut && <span className="fp-num fp-sub">{item.shortcut}</span>}
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
  onShowObs,
  onShowHelp,
  onToggleLive,
  onHardReset,
  onStartAutoCalibrate,
  onStartMicLearn,
  onCaptureNoise,
  onHandleModeSwitch,
  onSetMainTab,
  onSnapshotSave,
  onExportMp4,
  videoExportLabel = 'Video (WebM)',
  onShareRecording,
  onDownloadWaveform,
  onOpenLink,
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
        { label: `Export ${videoExportLabel} (Canvas + Audio)`, action: onExportMp4, disabled: !isLive },
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
        { label: isLive ? 'Stop engine' : 'Start engine', shortcut: 'Space', action: onToggleLive },
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
        { label: 'Live view', action: () => onSetMainTab?.('live'), disabled: mainTab === 'live' },
        { label: 'Editor', action: () => onSetMainTab?.('editor'), disabled: mainTab === 'editor' },
        { separator: true },
        { label: isBypassed ? 'Disable Bypass (B)' : 'Enable Bypass (B)', action: () => setIsBypassed?.(!isBypassed) },
      ],
    },
    {
      id: 'ai',
      label: 'Voice',
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
        { label: 'Lumina Link…', action: onOpenLink, disabled: !onOpenLink },
        // In the browser the panel explains that OBS control needs the desktop app.
        { label: isTauri ? 'OBS Studio…' : 'OBS Studio… (desktop app)', action: onShowObs, disabled: !onShowObs },
        { label: 'Audio Device Settings', shortcut: 'Ctrl+,', action: onShowSettings },
      ],
    },
    {
      id: 'help',
      label: 'Help',
      items: [
        { label: 'TIWATON Setup Guide', action: onShowHelp },
        { label: 'Routing: OBS + Virtual Cable', action: onShowHelp },
      ],
    },
  ];

  return (
    <nav className="fp-menubar" aria-label="Menu" {...dragRegionProps}>
      {onGoHome && <button type="button" className="fp-menu-btn" onClick={onGoHome}>home</button>}
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
    </nav>
  );
}

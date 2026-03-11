import React from 'react';

export default function StudioStatusBar({
  isLive,
  isPlayingFile,
  audioStats,
  features,
  voiceActive,
  visualizerGateStatus,
}) {
  return (
    <div
      className="h-9 shrink-0 flex items-center px-4 gap-4 text-[10px] z-20 border-b border-slate-800/40"
      style={{ background: '#030710', fontFamily: 'JetBrains Mono, monospace' }}
    >
      <span className={`flex items-center gap-1.5 font-bold ${isLive || isPlayingFile ? 'text-[#00E676]' : 'text-slate-600'}`}>
        <span className={`w-2 h-2 rounded-full ${isLive || isPlayingFile ? 'bg-[#00E676] animate-pulse' : 'bg-slate-700'}`} />
        {isLive ? 'ENGINE ACTIVE' : isPlayingFile ? 'FILE PLAYBACK' : 'STANDBY'}
      </span>
      <div className="h-3 w-px bg-slate-800" />
      <span className="text-slate-600">SR: <span className="text-slate-300">{audioStats.sampleRate > 0 ? `${(audioStats.sampleRate / 1000).toFixed(1)}kHz` : '--'}</span></span>
      <span className="text-slate-600">BUFFER: <span className="text-slate-300">{audioStats.bufferSize || '--'}</span></span>
      <span className="text-slate-600">LATENCY: <span className="text-slate-300">{audioStats.latencyMs !== null ? `${audioStats.latencyMs.toFixed(1)}ms` : '--'}</span></span>
      <span className="text-slate-600">CPU: <span className={`${audioStats.cpuLoadPct !== null && audioStats.cpuLoadPct > 70 ? 'text-[#FFB020]' : 'text-slate-300'}`}>{audioStats.cpuLoadPct !== null ? `${audioStats.cpuLoadPct.toFixed(0)}%` : '--'}</span></span>
      <span className="text-slate-600">CB AVG: <span className="text-slate-300">{audioStats.callbackAvgMs !== null ? `${audioStats.callbackAvgMs.toFixed(2)}ms` : '--'}</span></span>
      <div className="h-3 w-px bg-slate-800" />
      <span className={features.denoise && isLive ? 'text-[var(--accent)]' : 'text-slate-700'}>HYPERGATE:{features.denoise ? 'ON' : 'OFF'}</span>
      <span className={features.pastorIsolation && isLive ? 'text-[var(--accent)]' : 'text-slate-700'}>ISOLATION:{features.pastorIsolation ? 'ON' : 'OFF'}</span>
      <span className={features.mastering && isLive ? 'text-[#00E676]' : 'text-slate-700'}>POLISH:{features.mastering ? 'ON' : 'OFF'}</span>
      <div className="h-3 w-px bg-slate-800 ml-auto" />
      <span className="text-slate-600 ml-0">VOICE: <span className={voiceActive ? 'text-[#00E676] font-bold' : 'text-slate-600'}>{voiceActive ? '92%' : '0%'}</span></span>
      <span className={`font-bold px-2 py-0.5 rounded text-[9px] ${visualizerGateStatus ? 'bg-[#FF5252]/15 text-[#FF5252]' : 'bg-[#00E676]/10 text-[#00E676]'}`}>
        {visualizerGateStatus ? 'GATE CLOSED' : 'GATE OPEN'}
      </span>
    </div>
  );
}

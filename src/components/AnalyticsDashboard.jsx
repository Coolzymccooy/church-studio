import React, { useState, useEffect } from 'react';
import { getAnalytics, getTotals, getRecentEvents, clearAnalytics } from '../utils/analytics';
import { Activity, Trash2, RefreshCw } from 'lucide-react';

const EVENT_LABELS = {
  session_start: { label: 'Sessions Started', icon: '🚀', color: 'var(--accent)' },
  session_end: { label: 'Sessions Ended', icon: '🏁', color: '#94A3B8' },
  engine_start: { label: 'Engine Started', icon: '▶️', color: '#00E676' },
  engine_stop: { label: 'Engine Stopped', icon: '⏹', color: '#FF5252' },
  feature_toggle: { label: 'Feature Toggles', icon: '⚙️', color: '#60A5FA' },
  preset_change: { label: 'Preset Changes', icon: '🎙', color: 'var(--accent)' },
  recording_start: { label: 'Recordings Started', icon: '⏺', color: '#FF5252' },
  recording_stop: { label: 'Recordings Stopped', icon: '⏹', color: '#FF5252' },
  export_wav: { label: 'WAV Exports', icon: '💾', color: '#00E676' },
  export_mp4: { label: 'MP4 Exports', icon: '🎬', color: '#60A5FA' },
  sermon_master: { label: 'Sermon Masters Run', icon: '🎙', color: 'var(--accent)' },
  bypass_toggle: { label: 'Bypass Toggles', icon: '🔄', color: '#94A3B8' },
  noise_calibrate: { label: 'Calibrations Run', icon: '📊', color: '#A78BFA' },
  snapshot_save: { label: 'Snapshots Saved', icon: '📸', color: '#60A5FA' },
  settings_open: { label: 'Settings Opened', icon: '⚙️', color: '#94A3B8' },
};

function formatTime(ts) {
  const d = new Date(ts);
  const now = new Date();
  const diff = now - d;
  if (diff < 60000) return 'just now';
  if (diff < 3600000) return Math.floor(diff / 60000) + 'm ago';
  if (diff < 86400000) return Math.floor(diff / 3600000) + 'h ago';
  return d.toLocaleDateString();
}

export default function AnalyticsDashboard() {
  const [totals, setTotals] = useState({});
  const [recent, setRecent] = useState([]);
  const [view, setView] = useState('summary'); // 'summary' | 'feed'

  const refresh = () => {
    setTotals(getTotals());
    setRecent(getRecentEvents(30));
  };

  useEffect(() => { refresh(); }, []);

  const totalEvents = Object.values(totals).reduce((a, b) => a + b, 0);
  const engineStarts = totals.engine_start || 0;
  const recordings = totals.recording_start || 0;
  const exports = (totals.export_wav || 0) + (totals.export_mp4 || 0);
  const sermonMasters = totals.sermon_master || 0;

  return (
    <div className="space-y-3">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-1.5">
          <Activity size={12} className="text-amber-400" />
          <span className="text-[9px] font-bold tracking-[0.15em] text-slate-500 uppercase">Activity Analytics</span>
        </div>
        <div className="flex gap-1">
          <button onClick={() => setView(v => v === 'summary' ? 'feed' : 'summary')} className="px-2 py-0.5 rounded border border-slate-700 text-[9px] text-slate-400 hover:text-white">
            {view === 'summary' ? 'Feed' : 'Summary'}
          </button>
          <button onClick={refresh} className="p-0.5 rounded border border-slate-700 text-slate-500 hover:text-white">
            <RefreshCw size={10} />
          </button>
          <button onClick={() => { clearAnalytics(); refresh(); }} className="p-0.5 rounded border border-slate-700 text-slate-600 hover:text-red-400" title="Clear analytics">
            <Trash2 size={10} />
          </button>
        </div>
      </div>

      {view === 'summary' && (
        <>
          {/* Stat cards */}
          <div className="grid grid-cols-2 gap-1.5">
            {[
              { label: 'Total Events', value: totalEvents, color: 'var(--accent)' },
              { label: 'Engine Starts', value: engineStarts, color: '#00E676' },
              { label: 'Recordings', value: recordings, color: '#FF5252' },
              { label: 'Exports', value: exports, color: '#60A5FA' },
              { label: 'Sermon Masters', value: sermonMasters, color: 'var(--accent)' },
              { label: 'Snapshots', value: totals.snapshot_save || 0, color: '#A78BFA' },
            ].map(({ label, value, color }) => (
              <div key={label} className="rounded-lg border border-slate-800 px-2.5 py-2" style={{ background: '#080E1F' }}>
                <div className="text-[14px] font-bold font-mono" style={{ color }}>{value}</div>
                <div className="text-[9px] text-slate-600">{label}</div>
              </div>
            ))}
          </div>

          {/* Top events breakdown */}
          <div className="rounded-lg border border-slate-800 overflow-hidden" style={{ background: '#080E1F' }}>
            <div className="px-3 py-1.5 border-b border-slate-800" style={{ background: 'rgba(255,255,255,0.02)' }}>
              <span className="text-[9px] font-bold text-slate-600 uppercase tracking-wider">All Activity</span>
            </div>
            <div className="p-2 space-y-1">
              {Object.entries(totals).length === 0 ? (
                <p className="text-[9px] text-slate-700 p-1">No activity recorded yet. Start the engine to begin.</p>
              ) : (
                Object.entries(totals)
                  .sort((a, b) => b[1] - a[1])
                  .map(([key, count]) => {
                    const def = EVENT_LABELS[key];
                    const maxCount = Math.max(...Object.values(totals));
                    const pct = maxCount > 0 ? (count / maxCount) * 100 : 0;
                    return (
                      <div key={key} className="flex items-center gap-2">
                        <span className="text-[10px] w-4 text-center">{def?.icon || '•'}</span>
                        <div className="flex-1">
                          <div className="flex justify-between items-center mb-0.5">
                            <span className="text-[9px] text-slate-400">{def?.label || key}</span>
                            <span className="text-[9px] font-mono text-slate-500">{count}</span>
                          </div>
                          <div className="h-1 bg-slate-800 rounded-full overflow-hidden">
                            <div
                              className="h-full rounded-full transition-all"
                              style={{ width: `${pct}%`, background: def?.color || '#94A3B8' }}
                            />
                          </div>
                        </div>
                      </div>
                    );
                  })
              )}
            </div>
          </div>
        </>
      )}

      {view === 'feed' && (
        <div className="rounded-lg border border-slate-800 overflow-hidden" style={{ background: '#080E1F' }}>
          <div className="p-2 space-y-1 max-h-48 overflow-y-auto">
            {recent.length === 0 ? (
              <p className="text-[9px] text-slate-700 p-1">No events yet.</p>
            ) : (
              recent.map((ev, i) => {
                const def = EVENT_LABELS[ev.name];
                return (
                  <div key={i} className="flex items-center gap-2 py-1 border-b border-slate-800/60">
                    <span className="text-[10px] shrink-0">{def?.icon || '•'}</span>
                    <span className="text-[9px] text-slate-400 flex-1">{def?.label || ev.name}</span>
                    {ev.data?.feature && <span className="text-[8px] text-slate-600">{ev.data.feature}</span>}
                    {ev.data?.preset && <span className="text-[8px] text-slate-600">{ev.data.preset}</span>}
                    <span className="text-[8px] text-slate-700 shrink-0">{formatTime(ev.ts)}</span>
                  </div>
                );
              })
            )}
          </div>
        </div>
      )}
    </div>
  );
}

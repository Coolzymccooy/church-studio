import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';

function TrimPlate({ t, formatTime }) {
  return (
    <div className="fp-mod" style={{ position: 'absolute', bottom: 16, left: '50%', transform: 'translateX(-50%)', zIndex: 20, width: 'min(760px, 95%)' }}>
      <div className="fp-mod-hd">
        <span>Trim</span>
        <span className="fp-num fp-sub">{formatTime(Math.floor(t.duration))}</span>
      </div>
      <div className="fp-mod-bd">
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
          <label>
            <span className="fp-row"><span>Start</span><span className="fp-num">{formatTime(Math.floor(t.start || 0))}</span></span>
            <input type="range" className="fp-range" min="0" max={t.duration || 0} step="0.1" value={t.start} onChange={t.onStart} />
          </label>
          <label>
            <span className="fp-row"><span>End</span><span className="fp-num">{formatTime(Math.floor(t.end || t.duration || 0))}</span></span>
            <input type="range" className="fp-range" min="0" max={t.duration || 0} step="0.1" value={t.end} onChange={t.onEnd} />
          </label>
        </div>
        <div className="fp-keys">
          <Key onClick={t.onPreview} disabled={t.previewing} lit={t.previewing ? 'green' : null}>{t.previewing ? 'previewing' : 'preview'}</Key>
          <Key onClick={t.onClear}>full file</Key>
          <Key onClick={t.onExport} disabled={!t.canExport || t.exportStatus === 'processing'} lit={t.exportStatus === 'done' ? 'green' : null}>
            {t.exportStatus === 'done' ? 'mastered' : t.exportStatus === 'processing' ? 'processing' : 'process & export'}
          </Key>
        </div>
      </div>
    </div>
  );
}

function ReviewChip({ v }) {
  if (!v.isLive) return null;
  if (v.recordingState === 'recording') {
    return <span className="fp-chip" style={{ bottom: 56, left: '50%', transform: 'translateX(-50%)' }}><Lamp tone="r">recording check</Lamp></span>;
  }
  if (v.recordingState !== 'review') return null;
  const time = v.isPlayingBack
    ? `${v.formatTime(v.playbackPosition)} / ${v.playbackDuration ? v.formatTime(v.playbackDuration) : '--:--'}`
    : v.playbackDuration ? v.formatTime(v.playbackDuration) : '';
  return (
    <span className="fp-chip" style={{ bottom: 56, left: '50%', transform: 'translateX(-50%)' }}>
      <Lamp tone="a">mic muted · reviewing</Lamp>
      {time && <span className="fp-num">{time}</span>}
    </span>
  );
}

/** The live tab: the visualiser canvas, its state chips, the sound-check and error cards, and file trim. */
export default function LiveView({ v, canvasRef }) {
  return (
    <div className="flex-1 relative overflow-hidden">
      {!v.isLive && !v.isPlayingFile && v.mode === 'live' && (
        <div className="fp-overlay">
          <div className="fp-card">
            <h3>Sound check</h3>
            <span className="fp-sub">Engine off</span>
            <Key lit="green" className="fp-key-tall" onClick={v.onGoLive} style={{ minWidth: 120 }}>start engine</Key>
          </div>
        </div>
      )}
      {v.mode === 'file' && v.fileInfo && (
        <span className="fp-chip" style={{ top: 12, left: 12 }}>file · {v.fileInfo.name}</span>
      )}
      {v.isBypassed && (
        <span className="fp-chip" style={{ top: 12, left: '50%', transform: 'translateX(-50%)' }}>
          <Lamp tone="a">bypass · raw audio</Lamp>
        </span>
      )}
      {v.voiceActive && !v.isBypassed && (
        <span className="fp-chip" style={{ top: 12, right: 12 }}><Lamp tone="g">speech</Lamp></span>
      )}
      <ReviewChip v={v} />
      {v.isLive && v.engineError && (
        <div className="fp-overlay">
          <div className="fp-card" role="alert">
            <Lamp tone="r">engine failed</Lamp>
            <p className="fp-sub" style={{ margin: 0, whiteSpace: 'pre-line' }}>{v.engineError}</p>
            <Key onClick={v.onErrorBack}>back to sound check</Key>
          </div>
        </div>
      )}
      <canvas ref={canvasRef} className="block w-full h-full" role="img" aria-label="Live analyser: spectrum, gate state and level history" />
      {v.mode === 'file' && v.fileInfo && v.trim.duration > 0 && <TrimPlate t={v.trim} formatTime={v.formatTime} />}
    </div>
  );
}

import { useMemo, useState } from 'react';

import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';

import {
  EXPORT_FORMATS,
  defaultExportTrack,
  exportTracks,
  formatExportPercent,
  getDesktopExportController,
} from '../../lib/audioExport.js';
import { useAudioExport } from './useAudioExport.js';

/**
 * The "saved …" row's export keys: pick a track of the last recording
 * (the Stream mix by default) and save it as mp3 or m4a next to its WAV.
 */
export default function RecordExportKeys({ summary }) {
  const controller = useMemo(() => getDesktopExportController(), []);
  const exporter = useAudioExport(controller);
  const tracks = useMemo(() => exportTracks(summary), [summary]);
  const [picked, setPicked] = useState(null);
  const track = tracks.some((t) => t.file === picked) ? picked : defaultExportTrack(tracks);

  if (!track) return null;
  const busy = exporter.job !== null;
  const exportAs = (format) => exporter.run(format, () => controller.exportTrack({ track, format }));

  return (
    <>
      {tracks.length > 1 && (
        <select className="fp-field" aria-label="Track to export" value={track} disabled={busy}
          onChange={(event) => { setPicked(event.target.value); exporter.clearResult(); }}>
          {tracks.map((t) => <option key={t.file} value={t.file}>{t.name}</option>)}
        </select>
      )}
      {EXPORT_FORMATS.map((format) => (
        <Key key={format} onClick={() => exportAs(format)} disabled={busy}
          lit={exporter.job?.format === format ? 'orange' : null}
          aria-label={`export ${format}`}>
          {exporter.job?.format === format ? formatExportPercent(exporter.fraction) : format}
        </Key>
      ))}
      {exporter.result && !busy && (
        <span className="fp-sub" title={exporter.result.path}>{exporter.result.format} saved</span>
      )}
      <InfoTip label="Export">
        mp3 at 192 kbps for podcasts and WhatsApp; m4a (AAC) for phones and video editors. Saved next to the WAVs with the same name.
      </InfoTip>
      {exporter.error && (
        <span role="alert" className="fp-fault">
          {exporter.error}
          {' '}
          <Key onClick={exporter.dismissError} aria-label="Dismiss export error">dismiss</Key>
        </span>
      )}
    </>
  );
}

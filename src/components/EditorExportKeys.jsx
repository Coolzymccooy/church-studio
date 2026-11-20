import { useMemo } from 'react';

import Key from './faceplate/Key.jsx';

import { isTauri, showSaveDialog } from '../lib/platform';
import {
  EXPORT_FORMATS,
  editorExportName,
  encodeWavPcm16,
  exportFilter,
  formatExportPercent,
  getDesktopExportController,
} from '../lib/audioExport.js';
import { useAudioExport } from './recorder/useAudioExport.js';

function bufferChannels(audioBuffer) {
  const channels = [];
  for (let ch = 0; ch < audioBuffer.numberOfChannels; ch++) channels.push(audioBuffer.getChannelData(ch));
  return channels;
}

/** The editor's "export mp3" / "export m4a" keys (desktop app only). */
export default function EditorExportKeys({ audioBuffer }) {
  if (!isTauri) return null;
  return <DesktopEditorExportKeys audioBuffer={audioBuffer} />;
}

function DesktopEditorExportKeys({ audioBuffer }) {
  const controller = useMemo(() => getDesktopExportController(), []);
  const exporter = useAudioExport(controller);
  const busy = exporter.job !== null;

  const exportAs = async (format) => {
    if (!audioBuffer || busy) return;
    const dest = await showSaveDialog({ defaultName: editorExportName(format), filters: [exportFilter(format)] })
      .catch(() => null);
    if (!dest) return;
    await exporter.run(format, () => {
      const wavBytes = new Uint8Array(encodeWavPcm16(bufferChannels(audioBuffer), audioBuffer.sampleRate));
      return controller.exportEditorAudio({ wavBytes, format, dest });
    });
  };

  return (
    <>
      {EXPORT_FORMATS.map((format) => (
        <Key key={format} onClick={() => exportAs(format)} disabled={!audioBuffer || busy}
          lit={exporter.job?.format === format ? 'orange' : null}>
          {exporter.job?.format === format ? formatExportPercent(exporter.fraction) : `export ${format}`}
        </Key>
      ))}
      {exporter.result && !busy && (
        <span className="fp-sub" title={exporter.result.path}>{exporter.result.format} saved</span>
      )}
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

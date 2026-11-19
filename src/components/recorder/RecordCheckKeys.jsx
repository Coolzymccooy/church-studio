import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';

/**
 * Browser build only: the Web Audio record check of the processed stream.
 * idle → "record check"; recording → lamp, time and stop; review → play, discard and exports.
 */
export default function RecordCheckKeys({ state, duration, videoLabel, onToggle, onDiscard, onShare, onExportVideo }) {
  if (state === 'recording') {
    return (
      <div className="fp-keys" role="group" aria-label="Record check">
        <Lamp tone="r"><span className="fp-num">rec {duration}</span></Lamp>
        <Key lit="red" onClick={onToggle}>stop</Key>
      </div>
    );
  }
  if (state === 'review') {
    return (
      <div className="fp-keys" role="group" aria-label="Record check">
        <Key lit="green" onClick={onToggle}>play</Key>
        <Key onClick={onDiscard}>discard</Key>
        <Key onClick={() => onShare('webm')}>webm</Key>
        <Key onClick={() => onShare('wav')}>wav</Key>
        <Key onClick={onExportVideo}>{videoLabel}</Key>
      </div>
    );
  }
  return <Key onClick={onToggle}>record check</Key>;
}

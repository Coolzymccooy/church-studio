import React, { useState } from 'react';
import { Circle, FolderOpen, Square } from 'lucide-react';

import { getDesktopRecorderController, quickRecordView } from '../../lib/recorder.js';
import { useRecorder } from './useRecorder.js';

/**
 * DesktopRecordButton: the centre-bar record button in the desktop app.
 * Starts and stops the native multitrack recorder (every armed strip plus
 * the Stream mix). The browser build keeps the Web Audio "Record Check".
 */
export default function DesktopRecordButton() {
  const recorder = useRecorder(getDesktopRecorderController());
  const [confirmStop, setConfirmStop] = useState(false);
  const view = quickRecordView(recorder.status, recorder.error, confirmStop);

  const onPrimary = async () => {
    if (view.mode === 'recording') {
      if (!confirmStop) {
        setConfirmStop(true);
        return;
      }
      setConfirmStop(false);
      await recorder.stop();
      return;
    }
    setConfirmStop(false);
    await recorder.start(null);
  };

  return (
    <div className="flex items-center gap-1.5">
      <button
        type="button"
        onClick={onPrimary}
        disabled={recorder.busy || view.disabled}
        title={view.title}
        className={`flex items-center gap-1.5 px-2.5 py-1 rounded-full font-bold text-[9px] text-white disabled:opacity-50 ${
          view.mode === 'recording' ? 'bg-red-900/70 border border-red-500' : 'bg-red-600 hover:bg-red-500'
        }`}
      >
        {view.mode === 'recording'
          ? <Square size={7} fill="currentColor" />
          : <Circle size={7} fill="currentColor" />}
        {view.label}
      </button>
      {view.canOpenFolder && (
        <button
          type="button"
          onClick={recorder.openFolder}
          title="Open the last recording's folder"
          className="flex items-center gap-1 px-2 py-1 rounded-full text-[9px] text-slate-300 border border-slate-600 hover:text-white"
        >
          <FolderOpen size={10} /> Open folder
        </button>
      )}
      {view.message && (
        <span className={`text-[9px] ${view.messageLevel === 'error' ? 'text-red-400' : 'text-slate-400'}`}>
          {view.message}
        </span>
      )}
    </div>
  );
}

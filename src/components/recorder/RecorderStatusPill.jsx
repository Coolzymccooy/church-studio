import React from 'react';

import { isTauri } from '../../lib/platform';
import { getDesktopRecorderController, recorderPillText } from '../../lib/recorder.js';
import { useRecorderStatus } from './useRecorder.js';

/** Red "● REC 0:12:34" pill for the top status bar; hidden unless recording. */
export default function RecorderStatusPill() {
  const [status] = useRecorderStatus(isTauri ? getDesktopRecorderController() : null);
  const text = recorderPillText(status);
  if (!text) return null;
  const title = status.droppedFrames > 0
    ? `Multitrack recording · ${status.droppedFrames} dropped frames`
    : 'Multitrack recording';
  return (
    <span title={title} className="flex items-center gap-1 font-bold px-2 py-0.5 rounded text-[9px] bg-[#FF5252]/15 text-[#FF5252]">
      <span className="w-2 h-2 rounded-full bg-[#FF5252] animate-pulse" />
      {text}
    </span>
  );
}

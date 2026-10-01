import React from 'react';

import { isTauri } from '../../lib/platform';
import { getDesktopObsController, obsPillText } from '../../lib/obsControl.js';
import { useObsStatus } from './useObsStatus.js';

/** Compact "OBS ● LIVE / REC" pill for the top status bar; hidden unless connected. */
export default function ObsStatusPill() {
  const status = useObsStatus(isTauri ? getDesktopObsController() : null);
  const text = obsPillText(status);
  if (!text) return null;
  const live = status.streaming || status.recording;
  const title = [
    status.currentScene ? `Scene: ${status.currentScene}` : null,
    status.streamTimecode ? `Stream ${status.streamTimecode}` : null,
    status.recordTimecode ? `Rec ${status.recordTimecode}` : null,
  ].filter(Boolean).join(' · ') || 'Connected to OBS Studio';

  return (
    <span
      title={title}
      className={`font-bold px-2 py-0.5 rounded text-[9px] ${
        live ? 'bg-[#FF5252]/15 text-[#FF5252]' : 'bg-[#00E676]/10 text-[#00E676]'
      }`}
    >
      {text}
    </span>
  );
}

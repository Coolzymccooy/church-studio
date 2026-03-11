import test from 'node:test';
import assert from 'node:assert/strict';

import { getExportAvailability } from './exportFlow.js';

test('getExportAvailability enables review exports only when review audio exists', () => {
  const state = getExportAvailability({
    isLive: false,
    recordingState: 'review',
    exportStatus: null,
    fileExportStatus: null,
    hasRecordedAudio: true,
    hasLoadedFile: false,
  });

  assert.equal(state.canExportRecording, true);
  assert.equal(state.canExportMp4, false);
});

test('getExportAvailability blocks file export while processing', () => {
  const state = getExportAvailability({
    isLive: true,
    recordingState: 'idle',
    exportStatus: 'sharing',
    fileExportStatus: 'processing',
    hasRecordedAudio: false,
    hasLoadedFile: true,
  });

  assert.equal(state.canExportRecording, false);
  assert.equal(state.canExportMp4, false);
  assert.equal(state.canExportFileWav, false);
});

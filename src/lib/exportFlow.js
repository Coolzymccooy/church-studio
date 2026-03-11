export function getExportAvailability({
  isLive,
  recordingState,
  exportStatus,
  fileExportStatus,
  hasRecordedAudio,
  hasLoadedFile,
}) {
  return {
    canExportRecording:
      recordingState === 'review' && hasRecordedAudio && exportStatus !== 'sharing',
    canExportMp4:
      isLive && exportStatus !== 'sharing',
    canExportFileWav:
      hasLoadedFile && fileExportStatus !== 'processing',
  };
}

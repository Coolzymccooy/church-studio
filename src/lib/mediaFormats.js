// Picks the video container/codec MediaRecorder will actually produce in the
// current browser, so the UI can label exports honestly instead of always
// saying "MP4" and silently handing the user a .webm file when MP4 isn't
// supported (which is still the common case in Firefox, for example).

const VIDEO_CANDIDATES = [
  { mimeType: 'video/mp4;codecs=avc1.42E01E,mp4a.40.2', extension: 'mp4', label: 'Video (MP4)' },
  { mimeType: 'video/mp4;codecs=avc1,opus', extension: 'mp4', label: 'Video (MP4)' },
  { mimeType: 'video/mp4', extension: 'mp4', label: 'Video (MP4)' },
  { mimeType: 'video/webm;codecs=vp9,opus', extension: 'webm', label: 'Video (WebM)' },
  { mimeType: 'video/webm;codecs=vp8,opus', extension: 'webm', label: 'Video (WebM)' },
  { mimeType: 'video/webm', extension: 'webm', label: 'Video (WebM)' },
];

const FALLBACK_FORMAT = { mimeType: 'video/webm', extension: 'webm', label: 'Video (WebM)' };

/**
 * @param {(mimeType: string) => boolean} [isTypeSupported] Usually
 *   `MediaRecorder.isTypeSupported`, bound. Passed in (rather than read from
 *   the global) so this stays a pure, unit-testable function and callers can
 *   guard for environments where `MediaRecorder` doesn't exist at all.
 * @returns {{ mimeType: string, extension: 'mp4' | 'webm', label: string }}
 */
export function pickVideoMimeType(isTypeSupported) {
  if (typeof isTypeSupported === 'function') {
    for (const candidate of VIDEO_CANDIDATES) {
      if (isTypeSupported(candidate.mimeType)) return candidate;
    }
  }
  return FALLBACK_FORMAT;
}

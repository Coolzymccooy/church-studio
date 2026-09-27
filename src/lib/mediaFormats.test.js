import test from 'node:test';
import assert from 'node:assert/strict';

import { pickVideoMimeType } from './mediaFormats.js';

test('pickVideoMimeType prefers MP4 when the browser supports it', () => {
  const isTypeSupported = (mime) => mime.startsWith('video/mp4');
  const format = pickVideoMimeType(isTypeSupported);

  assert.equal(format.extension, 'mp4');
  assert.equal(format.label, 'Video (MP4)');
  assert.match(format.mimeType, /^video\/mp4/);
});

test('pickVideoMimeType falls back to WebM when MP4 is unsupported', () => {
  const isTypeSupported = (mime) => mime.startsWith('video/webm');
  const format = pickVideoMimeType(isTypeSupported);

  assert.equal(format.extension, 'webm');
  assert.equal(format.label, 'Video (WebM)');
  assert.match(format.mimeType, /^video\/webm/);
});

test('pickVideoMimeType prefers the more specific/higher quality codec string first', () => {
  const supported = ['video/mp4;codecs=avc1.42E01E,mp4a.40.2', 'video/mp4', 'video/webm'];
  const isTypeSupported = (mime) => supported.includes(mime);
  const format = pickVideoMimeType(isTypeSupported);

  assert.equal(format.mimeType, 'video/mp4;codecs=avc1.42E01E,mp4a.40.2');
});

test('pickVideoMimeType falls back to WebM when nothing is reported as supported', () => {
  const format = pickVideoMimeType(() => false);
  assert.equal(format.extension, 'webm');
  assert.equal(format.label, 'Video (WebM)');
});

test('pickVideoMimeType falls back to WebM when MediaRecorder is unavailable', () => {
  const format = pickVideoMimeType(undefined);
  assert.equal(format.extension, 'webm');
  assert.equal(format.label, 'Video (WebM)');
});

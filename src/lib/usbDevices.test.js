import test from 'node:test';
import assert from 'node:assert/strict';

import {
  PAIR_KEYS,
  buildInputChannels,
  buildOutputPairs,
  clockLamp,
  countOptions,
  deviceAttention,
  deviceOptionLabel,
  firstChannelOptions,
  isMissing,
  linkFor,
  linkLamp,
  pairLabel,
  pairOptions,
  showPairPicker,
  trackUnderruns,
  withDeviceChange,
} from './usbDevices.js';

const umc = { deviceId: 'UMC404HD 192k', label: 'UMC404HD 192k', channels: 4, transport: 'usb' };
const phones = { deviceId: 'Jabra', label: 'Jabra', channels: 2, transport: 'usb' };
const speakers = { deviceId: 'MacBook Pro Speakers', label: 'MacBook Pro Speakers', channels: 2, transport: 'builtin' };

test('option labels name the device, its channels when more than two, and usb', () => {
  assert.equal(deviceOptionLabel(umc), 'UMC404HD 192k · 4 ch · usb');
  assert.equal(deviceOptionLabel(phones), 'Jabra · usb');
  assert.equal(deviceOptionLabel(speakers), 'MacBook Pro Speakers');
  assert.equal(deviceOptionLabel({ deviceId: 'x', label: '' }), 'x');
});

test('pairs are offered only as far as the device goes', () => {
  assert.deepEqual(pairOptions(2), [{ value: 0, label: '1-2' }]);
  assert.deepEqual(pairOptions(1), [{ value: 0, label: '1-2' }]);
  assert.deepEqual(pairOptions(4).map((o) => o.label), ['1-2', '3-4']);
  assert.deepEqual(pairOptions(5).map((o) => o.label), ['1-2', '3-4']);
  assert.equal(pairOptions(18).length, 9);
  assert.equal(pairLabel(2), '5-6');
});

test('a saved device that is not listed is missing; placeholders never are', () => {
  const devices = [umc, phones];
  assert.equal(isMissing(devices, 'UMC404HD 192k'), false);
  assert.equal(isMissing(devices, '3:UMC404HD 192k'), false);
  assert.equal(isMissing(devices, 'Behringer X-Air'), true);
  for (const placeholder of ['default', 'Not set', 'Same as monitor', null, undefined, '']) {
    assert.equal(isMissing(devices, placeholder), false);
  }
});

test('output pairs reach the engine as clean integers per bus', () => {
  assert.deepEqual(
    buildOutputPairs({ mainPair: 0, streamPair: '1', monitorPair: -3, monitor2Pair: 2.7, monitor3Pair: undefined }),
    { main: 0, stream: 1, monitor: 0, monitor2: 0, monitor3: 0 },
  );
  assert.deepEqual(Object.values(PAIR_KEYS), ['mainPair', 'streamPair', 'monitorPair', 'monitor2Pair', 'monitor3Pair']);
});

test('the input range reaches the engine as first and count', () => {
  assert.deepEqual(buildInputChannels({}), { first: 0, count: null });
  assert.deepEqual(buildInputChannels({ inputFirst: 2, inputCount: 4 }), { first: 2, count: 4 });
  assert.deepEqual(buildInputChannels({ inputFirst: 'x', inputCount: 0 }), { first: 0, count: null });
});

test('channel choices follow the input device', () => {
  assert.deepEqual(firstChannelOptions(4).map((o) => o.label), ['1', '2', '3', '4']);
  assert.deepEqual(countOptions(4, 2).map((o) => o.label), ['all', '1', '2']);
  assert.deepEqual(countOptions(4, 2).map((o) => o.value), [null, 1, 2]);
});

const status = [
  { role: 'input', name: 'UMC404HD 192k', state: 'ok', clock: 'shared', underruns: 0 },
  { role: 'main', name: 'UMC404HD 192k', state: 'ok', clock: 'shared', pair: 0, underruns: 0 },
  { role: 'monitor', name: 'Jabra', state: 'ok', clock: 'corrected', pair: 0, underruns: 3 },
];

test('each role finds its link and lamp', () => {
  assert.equal(linkFor(status, 'monitor').name, 'Jabra');
  assert.equal(linkFor(status, 'monitor2'), null);
  assert.deepEqual(linkLamp({ state: 'ok' }), { tone: 'g', word: 'on' });
  assert.deepEqual(linkLamp({ state: 'waiting' }), { tone: 'a', word: 'waiting' });
  assert.deepEqual(linkLamp({ state: 'lost' }), { tone: 'r', word: 'lost' });
  assert.deepEqual(linkLamp({ state: 'failed' }), { tone: 'r', word: 'failed' });
  assert.equal(linkLamp(null), null);
});

test('the header lamp shows only what needs attention, worst first', () => {
  assert.equal(deviceAttention(status, false), null);
  assert.deepEqual(deviceAttention(status, true), { tone: 'a', word: 'clock slips' });
  const waiting = [...status, { role: 'monitor2', name: 'Pastor IEM', state: 'waiting' }];
  assert.deepEqual(deviceAttention(waiting, false), { tone: 'a', word: 'Pastor IEM waiting' });
  const lost = [{ ...status[0], state: 'lost' }, ...waiting.slice(1)];
  assert.deepEqual(deviceAttention(lost, true), { tone: 'r', word: 'UMC404HD 192k lost' });
  // One device on two buses is named once.
  const both = [status[0], { ...status[1], state: 'failed' }, { role: 'stream', name: 'UMC404HD 192k', state: 'failed' }];
  assert.deepEqual(deviceAttention(both, false), { tone: 'r', word: 'UMC404HD 192k failed' });
  assert.equal(deviceAttention([], false), null);
});

test('clock slips only while underruns rose in the last 10 s', () => {
  let history = [];
  let slipping;
  ({ history, slipping } = trackUnderruns(history, status, 0));
  assert.equal(slipping, false);
  ({ history, slipping } = trackUnderruns(history, status, 1000));
  assert.equal(slipping, false);
  const more = status.map((d) => (d.role === 'monitor' ? { ...d, underruns: 5 } : d));
  ({ history, slipping } = trackUnderruns(history, more, 2000));
  assert.equal(slipping, true);
  ({ history, slipping } = trackUnderruns(history, more, 13_000));
  assert.equal(slipping, false);
  assert.ok(history.length <= 12);
  // An engine restart (counts back to 0) is not a slip.
  ({ slipping } = trackUnderruns(history, status, 14_000));
  assert.equal(slipping, false);
});

test('the clock lamp says shared, synced or slips', () => {
  assert.equal(clockLamp([], false), null);
  assert.deepEqual(clockLamp(status.slice(0, 2), false), { tone: 'g', word: 'clock shared' });
  assert.deepEqual(clockLamp(status, false), { tone: 'g', word: 'clock synced' });
  assert.deepEqual(clockLamp(status, true), { tone: 'a', word: 'clock slips' });
});

test('changing a device resets its pair or input range', () => {
  const draft = { outputId: 'UMC', monitorPair: 1, inputId: 'X-Air', inputFirst: 8, inputCount: 2, mainPair: 1 };
  assert.deepEqual(withDeviceChange(draft, 'outputId', 'Jabra'), { ...draft, outputId: 'Jabra', monitorPair: 0 });
  assert.deepEqual(
    withDeviceChange(draft, 'inputId', 'Mic'),
    { ...draft, inputId: 'Mic', inputFirst: 0, inputCount: null },
  );
  assert.deepEqual(withDeviceChange(draft, 'broadcastBus', 'UMC'), { ...draft, broadcastBus: 'UMC', streamPair: 0 });
  assert.notEqual(withDeviceChange(draft, 'outputId', 'Jabra'), draft);
});

test('a pair picker shows for a wide device, or while a saved pair does not fit', () => {
  assert.equal(showPairPicker({ channels: 4 }, 0), true);
  assert.equal(showPairPicker({ channels: 2 }, 0), false);
  assert.equal(showPairPicker({ channels: 2 }, 1), true);
  assert.equal(showPairPicker(null, 1), false);
});

test('underruns while a device is down are not clock slips', () => {
  const ok = [{ role: 'monitor', name: 'Jabra', state: 'ok', clock: 'corrected', underruns: 0 }];
  let { history } = trackUnderruns([], ok, 0);
  ({ history } = trackUnderruns(history, [{ role: 'input', name: 'UMC', state: 'lost' }, { ...ok[0], underruns: 1 }], 1000));
  const { slipping } = trackUnderruns(history, [{ ...ok[0], underruns: 1 }], 2000);
  assert.equal(slipping, false);
});

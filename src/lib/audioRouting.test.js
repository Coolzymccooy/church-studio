import test from 'node:test';
import assert from 'node:assert/strict';

import {
  buildNativeEngineArgs,
  describeBroadcastRoute,
  devicesEqual,
  mixerRefreshKey,
  normalizeTauriDevices,
  reconcileSelectedDevices,
  resolveBroadcastLabel,
  resolveDeviceLabel,
} from './audioRouting.js';

test('normalizeTauriDevices maps native payload to UI device objects', () => {
  const devices = normalizeTauriDevices([
    { id: '0:USB Mic', name: 'USB Mic', is_default: true, channels: 2, transport: 'usb' },
    { id: 'Old', name: 'Old', is_default: false },
  ]);

  assert.deepEqual(devices, [
    { deviceId: '0:USB Mic', label: 'USB Mic', isDefault: true, channels: 2, transport: 'usb' },
    { deviceId: 'Old', label: 'Old', isDefault: false, channels: 0, transport: 'other' },
  ]);
});

test('reconcileSelectedDevices keeps a missing device so it is found again by name', () => {
  const next = reconcileSelectedDevices(
    {
      inputId: 'missing-input',
      outputId: 'missing-output',
      broadcastBus: 'missing-broadcast',
    },
    {
      inputs: [{ deviceId: 'input-a', label: 'Mic A' }],
      outputs: [{ deviceId: 'output-a', label: 'Speakers' }],
    },
  );

  assert.deepEqual(next, {
    inputId: 'missing-input',
    outputId: 'missing-output',
    broadcastBus: 'missing-broadcast',
    mainOutputId: 'Not set',
    monitor2OutputId: 'Not set',
    monitor3OutputId: 'Not set',
  });
});

test('reconcileSelectedDevices migrates legacy tauri ids to current device ids', () => {
  const next = reconcileSelectedDevices(
    {
      inputId: '2:USB Audio Codec',
      outputId: '4:USB Audio Codec',
      broadcastBus: '5:VB-CABLE Input',
    },
    {
      inputs: [{ deviceId: 'USB Audio Codec', label: 'USB Audio Codec' }],
      outputs: [
        { deviceId: 'USB Audio Codec', label: 'USB Audio Codec' },
        { deviceId: 'VB-CABLE Input', label: 'VB-CABLE Input' },
      ],
    },
  );

  assert.deepEqual(next, {
    inputId: 'USB Audio Codec',
    outputId: 'USB Audio Codec',
    broadcastBus: 'VB-CABLE Input',
    mainOutputId: 'Not set',
    monitor2OutputId: 'Not set',
    monitor3OutputId: 'Not set',
  });
});

test('buildNativeEngineArgs preserves dedicated monitor and broadcast outputs', () => {
  const args = buildNativeEngineArgs({
    inputId: 'mic-1',
    outputId: 'monitor-1',
    broadcastBus: 'broadcast-1',
  });

  assert.deepEqual(args, {
    inputDevice: 'mic-1',
    monitorOutputDevice: 'monitor-1',
    broadcastOutputDevice: 'broadcast-1',
    mainOutputId: null,
    monitor2OutputId: null,
    monitor3OutputId: null,
    outputPairs: { main: 0, stream: 0, monitor: 0, monitor2: 0, monitor3: 0 },
    inputChannels: { first: 0, count: null },
  });
});

test('buildNativeEngineArgs uses the camelCase names Tauri 2 matches', () => {
  const args = buildNativeEngineArgs({
    inputId: 'default',
    outputId: 'default',
    broadcastBus: 'Not set',
    mainOutputId: 'PA Out',
    monitor2OutputId: 'Not set',
    monitor3OutputId: 'Not set',
  });

  assert.deepEqual(Object.keys(args).sort(), [
    'broadcastOutputDevice',
    'inputChannels',
    'inputDevice',
    'mainOutputId',
    'monitor2OutputId',
    'monitor3OutputId',
    'monitorOutputDevice',
    'outputPairs',
  ]);
  assert.deepEqual(args, {
    inputDevice: null,
    monitorOutputDevice: null,
    broadcastOutputDevice: null,
    mainOutputId: 'PA Out',
    monitor2OutputId: null,
    monitor3OutputId: null,
    outputPairs: { main: 0, stream: 0, monitor: 0, monitor2: 0, monitor3: 0 },
    inputChannels: { first: 0, count: null },
  });
  assert.equal(
    buildNativeEngineArgs({ inputId: 'default', outputId: 'default', mainOutputId: 'default' })
      .mainOutputId,
    null,
  );
});

test('resolveBroadcastLabel mirrors monitor label when requested', () => {
  const outputs = [{ deviceId: 'monitor-1', label: 'Headphones' }];

  assert.equal(
    resolveBroadcastLabel(
      { outputId: 'monitor-1', broadcastBus: 'Same as monitor' },
      outputs,
    ),
    'Headphones',
  );
});

test('resolveDeviceLabel accepts legacy tauri ids that still contain the device label', () => {
  const outputs = [{ deviceId: 'USB Headset', label: 'USB Headset' }];

  assert.equal(resolveDeviceLabel(outputs, '7:USB Headset'), 'USB Headset');
});

test('describeBroadcastRoute reports a dedicated native broadcast route', () => {
  const route = describeBroadcastRoute(
    { outputId: 'monitor-1', broadcastBus: 'broadcast-1' },
    [
      { deviceId: 'monitor-1', label: 'Headphones' },
      { deviceId: 'broadcast-1', label: 'VB-CABLE Input' },
    ],
  );

  assert.deepEqual(route, {
    mode: 'dedicated-broadcast',
    monitor: 'Headphones',
    broadcast: 'VB-CABLE Input',
    hasNativeBroadcastRoute: true,
  });
});

test('reconcileSelectedDevices keeps a Main output, present or missing', () => {
  const outputs = [{ deviceId: 'pa-1', label: 'PA Interface' }];
  const kept = reconcileSelectedDevices(
    { inputId: 'default', outputId: 'default', broadcastBus: 'Not set', mainOutputId: 'PA Interface' },
    { inputs: [], outputs },
  );
  assert.equal(kept.mainOutputId, 'pa-1');
  assert.equal(buildNativeEngineArgs(kept).mainOutputId, 'pa-1');

  const dropped = reconcileSelectedDevices(
    { inputId: 'default', outputId: 'default', broadcastBus: 'Not set', mainOutputId: 'gone' },
    { inputs: [], outputs },
  );
  assert.equal(dropped.mainOutputId, 'gone');
  assert.equal(buildNativeEngineArgs(dropped).mainOutputId, 'gone');
});

test('mon 2 and mon 3 outputs reach the engine args and stay set while missing', () => {
  const selected = {
    inputId: 'default', outputId: 'default', broadcastBus: 'Not set', mainOutputId: 'Not set',
    monitor2OutputId: 'dev-b', monitor3OutputId: 'gone',
  };
  const args = buildNativeEngineArgs(selected);
  assert.equal(args.monitor2OutputId, 'dev-b');
  assert.equal(args.monitor3OutputId, 'gone');
  assert.equal(buildNativeEngineArgs({ ...selected, monitor2OutputId: 'Not set' }).monitor2OutputId, null);
  const next = reconcileSelectedDevices(selected, { inputs: [], outputs: [{ deviceId: 'dev-b', label: 'B' }] });
  assert.equal(next.monitor2OutputId, 'dev-b');
  assert.equal(next.monitor3OutputId, 'gone');
});

test('engine args carry the output pairs and the input channel range', () => {
  const args = buildNativeEngineArgs({
    inputId: 'X-Air', outputId: 'default', broadcastBus: 'UMC', mainOutputId: 'UMC',
    mainPair: 0, streamPair: 1, inputFirst: 2, inputCount: 8,
  });
  assert.deepEqual(args.outputPairs, { main: 0, stream: 1, monitor: 0, monitor2: 0, monitor3: 0 });
  assert.deepEqual(args.inputChannels, { first: 2, count: 8 });
});

test('devicesEqual sees a changed pair or input range', () => {
  const a = { inputId: 'x', outputId: 'y', broadcastBus: 'Not set', mainOutputId: 'Not set', monitor2OutputId: 'Not set', monitor3OutputId: 'Not set' };
  assert.equal(devicesEqual(a, { ...a, mainPair: 1 }), false);
  assert.equal(devicesEqual(a, { ...a, inputFirst: 2 }), false);
});

test('devicesEqual compares every device choice', () => {
  const a = { inputId: 'x', outputId: 'y', broadcastBus: 'Not set', mainOutputId: 'Not set', monitor2OutputId: 'm2', monitor3OutputId: 'Not set' };
  assert.equal(devicesEqual(a, { ...a }), true);
  assert.equal(devicesEqual(a, { ...a, monitor2OutputId: 'other' }), false);
});

test('mixerRefreshKey changes on start, stop and a new mon 2 / mon 3 device', () => {
  const route = { monitor: 'Jabra', broadcast: null, monitor2: null, monitor3: null };
  const before = mixerRefreshKey('running', route);
  assert.equal(mixerRefreshKey('running', { ...route }), before);
  assert.notEqual(mixerRefreshKey('running', { ...route, monitor2: 'AirPods' }), before);
  assert.notEqual(mixerRefreshKey('running', { ...route, monitor3: 'USB' }), before);
  assert.equal(mixerRefreshKey('stopped', route), 'stopped');
  assert.equal(mixerRefreshKey(undefined, null), 'stopped');
});

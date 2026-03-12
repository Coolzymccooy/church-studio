import test from 'node:test';
import assert from 'node:assert/strict';

import {
  buildNativeEngineArgs,
  describeBroadcastRoute,
  normalizeTauriDevices,
  reconcileSelectedDevices,
  resolveBroadcastLabel,
  resolveDeviceLabel,
} from './audioRouting.js';

test('normalizeTauriDevices maps native payload to UI device objects', () => {
  const devices = normalizeTauriDevices([
    { id: '0:USB Mic', name: 'USB Mic', is_default: true },
  ]);

  assert.deepEqual(devices, [
    { deviceId: '0:USB Mic', label: 'USB Mic', isDefault: true },
  ]);
});

test('reconcileSelectedDevices resets missing devices to safe defaults', () => {
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
    inputId: 'default',
    outputId: 'default',
    broadcastBus: 'Not set',
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
  });
});

test('buildNativeEngineArgs preserves dedicated monitor and broadcast outputs', () => {
  const args = buildNativeEngineArgs({
    inputId: 'mic-1',
    outputId: 'monitor-1',
    broadcastBus: 'broadcast-1',
  });

  assert.deepEqual(args, {
    input_device: 'mic-1',
    monitor_output_device: 'monitor-1',
    broadcast_output_device: 'broadcast-1',
  });
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

import { INPUT_RANGE_KEYS, PAIR_KEYS, buildInputChannels, buildOutputPairs } from './usbDevices.js';

export function normalizeTauriDevices(devices = []) {
  return devices.map((device) => ({
    deviceId: device.id,
    label: device.name,
    isDefault: device.is_default,
    channels: device.channels ?? 0,
    transport: device.transport ?? 'other',
  }));
}

function matchesDeviceSelection(selection, device) {
  if (!selection) return false;
  if (selection === device.deviceId || selection === device.label) return true;
  const legacyLabel = selection.includes(':') ? selection.split(':').slice(1).join(':') : null;
  return legacyLabel === device.label;
}

function resolveCurrentDeviceId(selection, devices, fallback) {
  const matched = devices.find((device) => matchesDeviceSelection(selection, device));
  return matched ? matched.deviceId : fallback;
}

/**
 * Bring saved device ids up to date with the listed devices (legacy "index:name" ids become
 * names). A saved device that is not connected keeps its id, so it is found again by name when
 * it is plugged back in (docs/specs/2026-11-20-usb-interfaces.md §2).
 */
export function reconcileSelectedDevices(selectedDevices, availableDevices) {
  const next = { ...selectedDevices };
  const { inputs = [], outputs = [] } = availableDevices;

  next.inputId = !next.inputId || next.inputId === 'default'
    ? 'default'
    : resolveCurrentDeviceId(next.inputId, inputs, next.inputId);
  next.outputId = !next.outputId || next.outputId === 'default'
    ? 'default'
    : resolveCurrentDeviceId(next.outputId, outputs, next.outputId);

  if (!next.broadcastBus) {
    next.broadcastBus = 'Not set';
  } else if (next.broadcastBus !== 'Not set' && next.broadcastBus !== 'Same as monitor') {
    next.broadcastBus = resolveCurrentDeviceId(next.broadcastBus, outputs, next.broadcastBus);
  }

  // The Main (PA) and the mon 2 / mon 3 outputs are desktop-only and optional.
  for (const key of OPTIONAL_OUTPUT_KEYS) {
    next[key] = resolveOptionalOutputId(next, key)
      ? resolveCurrentDeviceId(next[key], outputs, next[key])
      : 'Not set';
  }

  return next;
}

/** Optional desktop outputs: the Main (PA) bus and the mon 2 / mon 3 monitor mixes. */
export const OPTIONAL_OUTPUT_KEYS = ['mainOutputId', 'monitor2OutputId', 'monitor3OutputId'];

/** Every device choice the engine uses, for comparing two selections. */
export const DEVICE_KEYS = [
  'inputId', 'outputId', 'broadcastBus', ...OPTIONAL_OUTPUT_KEYS, ...Object.values(PAIR_KEYS), ...INPUT_RANGE_KEYS,
];

export function devicesEqual(a, b) {
  return DEVICE_KEYS.every((key) => (a?.[key] ?? null) === (b?.[key] ?? null));
}

/** The device id for an optional output, or null when it is not set. */
export function resolveOptionalOutputId(selectedDevices, key) {
  const id = selectedDevices?.[key];
  return id && id !== 'default' && id !== 'Not set' ? id : null;
}

export function resolveDeviceLabel(devices, deviceId, {
  defaultLabel = 'System Default',
  missingLabel = 'Custom Device',
} = {}) {
  if (!deviceId || deviceId === 'default') return defaultLabel;
  const found = devices.find((device) => matchesDeviceSelection(deviceId, device));
  return found?.label || missingLabel;
}

export function resolveBroadcastDeviceId(selectedDevices) {
  if (
    !selectedDevices.broadcastBus
    || selectedDevices.broadcastBus === 'Not set'
    || selectedDevices.broadcastBus === 'Same as monitor'
  ) {
    return null;
  }

  return selectedDevices.broadcastBus;
}

export function resolveBroadcastLabel(selectedDevices, outputs) {
  if (selectedDevices.broadcastBus === 'Same as monitor') {
    return resolveDeviceLabel(outputs, selectedDevices.outputId);
  }

  const broadcastDeviceId = resolveBroadcastDeviceId(selectedDevices);
  if (!broadcastDeviceId) return 'Not set';
  return resolveDeviceLabel(outputs, broadcastDeviceId, { missingLabel: 'Broadcast Device' });
}

export function resolveMainOutputId(selectedDevices) {
  const { mainOutputId } = selectedDevices;
  return mainOutputId && mainOutputId !== 'default' && mainOutputId !== 'Not set' ? mainOutputId : null;
}

// Tauri 2 matches command arguments by their camelCase names
// (Rust `input_device` -> `inputDevice`); snake_case keys are ignored.
export function buildNativeEngineArgs(selectedDevices) {
  return {
    inputDevice: selectedDevices.inputId !== 'default' ? selectedDevices.inputId : null,
    monitorOutputDevice: selectedDevices.outputId !== 'default' ? selectedDevices.outputId : null,
    broadcastOutputDevice: resolveBroadcastDeviceId(selectedDevices),
    mainOutputId: resolveMainOutputId(selectedDevices),
    monitor2OutputId: resolveOptionalOutputId(selectedDevices, 'monitor2OutputId'),
    monitor3OutputId: resolveOptionalOutputId(selectedDevices, 'monitor3OutputId'),
    outputPairs: buildOutputPairs(selectedDevices),
    inputChannels: buildInputChannels(selectedDevices),
  };
}

export function describeBroadcastRoute(selectedDevices, outputs) {
  const monitor = resolveDeviceLabel(outputs, selectedDevices.outputId);
  const broadcast = resolveBroadcastLabel(selectedDevices, outputs);
  const broadcastDeviceId = resolveBroadcastDeviceId(selectedDevices);

  if (selectedDevices.broadcastBus === 'Same as monitor') {
    return {
      mode: 'mirror-monitor',
      monitor,
      broadcast: monitor,
      hasNativeBroadcastRoute: selectedDevices.outputId !== 'default',
    };
  }

  if (broadcastDeviceId) {
    return {
      mode: 'dedicated-broadcast',
      monitor,
      broadcast,
      hasNativeBroadcastRoute: true,
    };
  }

  return {
    mode: 'monitor-only',
    monitor,
    broadcast: 'Not set',
    hasNativeBroadcastRoute: false,
  };
}

/**
 * A key for the mixer view that changes when the engine starts or stops or opens different
 * outputs, so the console fetches which buses have a device again.
 */
export function mixerRefreshKey(engineState, route) {
  const running = engineState === 'running';
  if (!running) return 'stopped';
  return ['running', route?.monitor, route?.monitor2, route?.monitor3, route?.broadcast]
    .map((v) => v ?? '')
    .join('|');
}

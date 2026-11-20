export function normalizeTauriDevices(devices = []) {
  return devices.map((device) => ({
    deviceId: device.id,
    label: device.name,
    isDefault: device.is_default,
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

export function reconcileSelectedDevices(selectedDevices, availableDevices) {
  const next = { ...selectedDevices };
  const { inputs = [], outputs = [] } = availableDevices;

  next.inputId = next.inputId === 'default'
    ? 'default'
    : resolveCurrentDeviceId(next.inputId, inputs, 'default');
  next.outputId = next.outputId === 'default'
    ? 'default'
    : resolveCurrentDeviceId(next.outputId, outputs, 'default');

  if (next.broadcastBus !== 'Not set' && next.broadcastBus !== 'Same as monitor') {
    next.broadcastBus = resolveCurrentDeviceId(next.broadcastBus, outputs, 'Not set');
  }

  // The Main (PA) and the mon 2 / mon 3 outputs are desktop-only and optional; a device that has
  // gone away falls back to "Not set" rather than to the system default.
  for (const key of OPTIONAL_OUTPUT_KEYS) {
    next[key] = resolveOptionalOutputId(next, key)
      ? resolveCurrentDeviceId(next[key], outputs, 'Not set')
      : 'Not set';
  }

  return next;
}

/** Optional desktop outputs: the Main (PA) bus and the mon 2 / mon 3 monitor mixes. */
export const OPTIONAL_OUTPUT_KEYS = ['mainOutputId', 'monitor2OutputId', 'monitor3OutputId'];

/** Every device choice the engine uses, for comparing two selections. */
export const DEVICE_KEYS = ['inputId', 'outputId', 'broadcastBus', ...OPTIONAL_OUTPUT_KEYS];

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

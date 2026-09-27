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

  // The Main (PA) output is desktop-only and optional; a device that has
  // gone away falls back to "Not set" rather than to the system default.
  next.mainOutputId = resolveMainOutputId(next)
    ? resolveCurrentDeviceId(next.mainOutputId, outputs, 'Not set')
    : 'Not set';

  return next;
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

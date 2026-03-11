export function normalizeTauriDevices(devices = []) {
  return devices.map((device) => ({
    deviceId: device.id,
    label: device.name,
    isDefault: device.is_default,
  }));
}

export function reconcileSelectedDevices(selectedDevices, availableDevices) {
  const next = { ...selectedDevices };
  const { inputs = [], outputs = [] } = availableDevices;

  const hasInput = next.inputId === 'default'
    || inputs.some((device) => device.deviceId === next.inputId);
  const hasOutput = next.outputId === 'default'
    || outputs.some((device) => device.deviceId === next.outputId);
  const hasBroadcast = next.broadcastBus === 'Not set'
    || next.broadcastBus === 'Same as monitor'
    || outputs.some((device) => device.deviceId === next.broadcastBus);

  if (!hasInput) next.inputId = 'default';
  if (!hasOutput) next.outputId = 'default';
  if (!hasBroadcast) next.broadcastBus = 'Not set';

  return next;
}

export function resolveDeviceLabel(devices, deviceId, {
  defaultLabel = 'System Default',
  missingLabel = 'Custom Device',
} = {}) {
  if (!deviceId || deviceId === 'default') return defaultLabel;
  const found = devices.find((device) => device.deviceId === deviceId);
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

export function buildNativeEngineArgs(selectedDevices) {
  return {
    input_device: selectedDevices.inputId !== 'default' ? selectedDevices.inputId : null,
    monitor_output_device: selectedDevices.outputId !== 'default' ? selectedDevices.outputId : null,
    broadcast_output_device: resolveBroadcastDeviceId(selectedDevices),
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

/** The scene `dir` (-1 or +1) steps away from `current` in `scenes`, wrapping; null if none. */
export function stepScene(scenes, current, dir) {
  if (!Array.isArray(scenes) || scenes.length === 0) return null;
  const i = scenes.indexOf(current);
  if (i < 0) return dir < 0 ? scenes[scenes.length - 1] : scenes[0];
  return scenes[(i + dir + scenes.length) % scenes.length];
}

/** Lamp for the plate header: connected (green), connecting (amber) or off. */
export function obsLampState(status) {
  if (status?.connected) return { tone: 'g', word: status.obsVersion ? `obs ${status.obsVersion}` : 'connected' };
  if (status?.connecting) return { tone: 'a', word: 'connecting' };
  return { tone: 'off', word: status?.enabled ? 'not connected' : 'off' };
}

/** Legend for an output key: the action when idle, the state and timecode while running. */
export function outputKey(kind, active, timecode) {
  if (kind === 'stream') return active ? { text: 'on air', tc: timecode || '' } : { text: 'go live', tc: '' };
  return active ? { text: 'recording', tc: timecode || '' } : { text: 'record', tc: '' };
}

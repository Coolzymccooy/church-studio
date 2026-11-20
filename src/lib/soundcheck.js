import { formatElapsed } from './recorder.js';

export function createSoundcheckController(invoke) {
  return {
    status: () => invoke('soundcheck_status'),
    load: (path) => invoke('soundcheck_load', { path }),
    unload: () => invoke('soundcheck_unload'),
    source: (index, playback) => invoke('soundcheck_set_source', { index, playback }),
    play: () => invoke('soundcheck_play'),
    stop: () => invoke('soundcheck_stop'),
    loop: (enabled) => invoke('soundcheck_set_loop', { enabled }),
    locate: (seconds) => invoke('soundcheck_locate', { seconds }),
  };
}

export function durationLabel(seconds) {
  const clock = formatElapsed(seconds);
  return clock.startsWith('0:') ? clock.slice(2).replace(/^0/, '') : clock;
}

export function soundcheckLamp(signal) {
  return signal?.playing ? 'g' : signal?.selected ? 'a' : 'off';
}
